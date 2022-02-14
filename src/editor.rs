use std::fs::File;
use hex::FromHex;
use pancurses::Window;
use anyhow::anyhow;
use crate::widget::{Direction, HexView};
use crate::options::Config;


/// Type of view.
pub enum ViewType {
    Hex,
}


/// The main editor object.
pub struct Editor {
    cur_view: ViewType,
    hex_view: HexView,
    seek: u64,
    win: Window,
    cmdline_win: Window,
    config: Config,
    status: String
}

impl Editor {
    /// Initialises the screen and returns a new Editor.
    pub fn init(file: File, options: Config) -> Self
    {
        let win = pancurses::initscr();
        pancurses::raw();
        pancurses::noecho();
        ncurses::set_escdelay(0);

        let (y, x) = win.get_max_yx();

        let mut editor = Self {
            cur_view: ViewType::Hex,
            hex_view: HexView::new(
                win.derwin(y - 1, x, 0, 0)
                    .expect("failed to create a subwin"),
                    file,
                    &options
            ),
            cmdline_win: win.derwin(1, x, y - 1, 0)
                .expect("failed to create a subwin"),
            status: String::new(),
            seek: 0,
            win,
            config: options
        };

        // Enable all keys.
        editor.win.keypad(true);

        editor.status.push_str("-- NORMAL --");

        // Seek to the start , draw and refresh the windows.
        editor.seek(0);
        editor.draw();
        editor.refresh();

        editor
    }

    /// Replaces the byte under the cursor and writes it to the file.
    pub fn replace(&mut self) -> anyhow::Result<u64>
    {
        let mut input = String::new();

        // Listen for 2 characters.
        for _ in 0..2 {
            match self.win.getch() {
                Some(pancurses::Input::Character(c)) => {
                    if c == 0x1b as char {
                        return Ok(0);
                    } else {
                        input.push(c);
                    }
                }
                _ => ()
            }
        }

        for c in input.chars() {
            if !c.is_ascii_hexdigit() {
                return Err(anyhow!("{}: invalid hex digit", c));
            }
        }

        let byte_buf: Vec<u8> = Vec::from_hex(&input)?;
        self.hex_view.write_byte_at_cursor(byte_buf[0])?;

        self.hex_view.read_buf().ok();
        self.hex_view.draw().ok();
        self.hex_view.refresh();

        Ok(1)
    }

    /// Replaces many bytes, until Esc is pressed.
    pub fn replace_many(&mut self) -> anyhow::Result<()>
    {
        let status_backup = self.status.clone();

        self.status.clear();
        self.status.push_str("-- REPLACE --");

        self.cmdline_win.clear();
        self.cmdline_win.printw(&self.status);
        let (y, x) = self.hex_view.get_cur_yx();
        self.win.mv(y, x);
        self.cmdline_win.refresh();

        // Replace bytes until ESC.
        loop {
            if self.replace()? == 0 {
                break;
            }
            self.move_cursor(Direction::Right, 1);
        }

        self.status = status_backup;
        self.draw();
        self.refresh();

        Ok(())
    }

    /// Invokes the command prompt, listens for keys, and returns teh input.
    pub fn prompt(&self) -> Option<String>
    {
        let (y, x) = self.win.get_cur_yx();
        let mut command = String::new();

        self.cmdline_win.clear();
        self.win.mv(self.cmdline_win.get_beg_y(), self.cmdline_win.get_beg_x() + 1);
        self.cmdline_win.addch(':');
        self.cmdline_win.refresh();

        loop {
            match self.win.getch() {
                Some(pancurses::Input::Character(c)) => {
                    if c == '\n' {
                        break;
                    } else if c == 0x1B as char {
                        command.clear();
                        break;
                    } else {
                        self.cmdline_win.addch(c);
                        self.cmdline_win.refresh();
                        command.push(c);
                    }
                },
                Some(pancurses::Input::KeyBackspace) => {
                    if command.len() < 1 {
                        break;
                    }

                    let (cur_y, cur_x) = self.cmdline_win.get_cur_yx();
                    self.cmdline_win.mv(cur_y, cur_x - 1);
                    self.cmdline_win.delch();
                    self.cmdline_win.refresh();
                    command.pop();
                }
                Some(_) => (),
                None => ()
            }
        }

        self.cmdline_win.clear();
        self.cmdline_win.printw(self.status.as_str());
        self.win.mv(y, x);

        if command.len() > 0 {
            Some(command)
        } else {
            None
        }
    }

    /// Draw the screen.
    fn draw(&mut self)
    {
        match self.cur_view {
            ViewType::Hex => {
                self.hex_view.draw().ok();
            }
        }

        self.cmdline_win.clear();
        self.cmdline_win.mv(0, 0);
        self.cmdline_win.printw(self.status.as_str());
    }

    /// Refresh the screen.
    pub fn refresh(&self)
    {
        match self.cur_view {
            ViewType::Hex => {
                self.hex_view.refresh();
                // The cursor of the main window needs to be set on every refresh, for some reason.
                let (y, x) = self.hex_view.get_cur_yx();
                self.win.mv(y, x);

            },
        }

        self.cmdline_win.refresh();
    }

    /// Finish
    pub fn end(&self)
    {
        pancurses::endwin();
    }

    /// Return the width of the entire screen.
    pub fn width(&self) -> i32
    {
        self.win.get_max_x()
    }

    /// Return the width of the entire screen.
    pub fn height(&self) -> i32
    {
        self.win.get_max_y()
    }

    /// Listen for an input event.
    pub fn getch(&self) -> Option<pancurses::Input>
    {
        self.win.getch()
    }

    /// Move the cursor.
    pub fn move_cursor(&mut self, direction: Direction, count: i32)
    {
        match self.cur_view {
            ViewType::Hex => {
                match self.hex_view.move_cursor(direction, count) {
                    Err(_) => (),
                    Ok(v) => self.seek = v
                }
                // Again, the cursor of the main window needs to be reset.
                let (y, x) = self.hex_view.get_cur_yx();
                self.win.mv(y, x);
            }
        }
    }

    /// Seek - jump to a 16-byte aligned offset, advancing the cursor properly.
    /// Accepts both positive and negative values - if negative, start from the end.
    pub fn seek(&mut self, offset: i64)
    {
        match self.cur_view {
            ViewType::Hex => {
                match self.hex_view.seek(offset) {
                    Err(_) => return (),
                    Ok(_) => ()
                }
                self.seek = offset as u64;
                let (y, x) = self.hex_view.get_cur_yx();
                self.win.mv(y, x);
            }
        }
    }

    /// Scrolls the view up and down.
    pub fn scroll(&mut self, direction: Direction, count: u32)
    {
        match self.cur_view {
            ViewType::Hex => {
                self.hex_view.scroll(direction, count).ok();
            },
        }
    }

    /// Switches the active pane of the current view.
    pub fn switch_pane(&mut self)
    {
        match self.cur_view {
            ViewType::Hex => {
                self.hex_view.switch_pane().ok();
            },
        }
    }

    // TODO
    /// Switches the view.
    pub fn switch_view(&mut self)
    {
        match self.cur_view {
            ViewType::Hex => {
                self.cur_view = ViewType::Hex;
            },
        }

        self.draw();
        self.refresh();
    }
}
