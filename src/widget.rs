use std::io::{Write, Seek, SeekFrom};
use std::fs::File;
use anyhow::{anyhow, bail};
use crate::options::Config;
use crate::util;

const OFFSET_PANE_WIDTH: i32 = 8;
const HEX_PANE_WIDTH: i32 = 32 + 7;
const CANON_PANE_WIDTH: i32 = 16;
const SEP_WIDTH: i32 = 3;

const SEP: &str = " | ";


/// Directions
pub enum Direction {
    Up,
    Down,
    Left,
    Right
}


/// Types of hex view panes.
enum HexPane {
    Hex,
    Canon,
}

/// Editing modes in the hex view.
enum HexEditingMode {
    Normal,
    Insert,
    Replace
}


/// The hex view object.
pub struct HexView {
    win: pancurses::Window,
    offset_win: pancurses::Window,
    hex_win: pancurses::Window,
    canon_win: pancurses::Window,
    statusline_win: pancurses::Window,
    status: String,
    oh_sep_win: pancurses::Window,
    hc_sep_win: pancurses::Window,
    cs_sep_win: pancurses::Window,
    file: File,
    active_pane: HexPane,
    position_y: i32,
    position_x: i32,
    edit_mode: HexEditingMode,
    buffer: Vec<u8>
}

impl HexView {
    /// Returns a new HexView.
    pub fn new(win: pancurses::Window, f: File, config: &Config) -> Self
    {
        let mut widget = Self {
            offset_win: win.derwin(
                win.get_max_y() - 1,
                OFFSET_PANE_WIDTH,
                0,
                0
            ).expect("failed to create a subwin"),
            hex_win: win.derwin(
                win.get_max_y() - 1,
                HEX_PANE_WIDTH,
                0,
                SEP_WIDTH + OFFSET_PANE_WIDTH
            ).expect("failed to create a subwin"),
            canon_win: win.derwin(
                win.get_max_y() - 1,
                CANON_PANE_WIDTH,
                0,
                (SEP_WIDTH * 2) + OFFSET_PANE_WIDTH + HEX_PANE_WIDTH
            ).expect("failed to create a subwin"),
            statusline_win: win.derwin(
                1,
                win.get_max_x(),
                win.get_max_y() - 1,
                0
            ).expect("failed to create a subwin"),
            oh_sep_win: win.derwin(
                win.get_max_y() - 1,
                SEP_WIDTH,
                0,
                OFFSET_PANE_WIDTH
            ).expect("failed to create a subwin"),
            hc_sep_win: win.derwin(
                win.get_max_y() - 1,
                SEP_WIDTH,
                0,
                OFFSET_PANE_WIDTH + SEP_WIDTH + HEX_PANE_WIDTH
            ).expect("failed to create a subwin"),
            cs_sep_win: win.derwin(
                win.get_max_y() - 1,
                SEP_WIDTH,
                0,
                OFFSET_PANE_WIDTH + (2 * SEP_WIDTH) + HEX_PANE_WIDTH + CANON_PANE_WIDTH
            ).expect("failed to create a subwin"),
            status: String::new(),
            active_pane: HexPane::Hex,
            win,
            position_y: 0,
            position_x: 0,
            edit_mode: HexEditingMode::Normal,
            file: f,
            buffer: Vec::new()
        };

        widget.status.push_str(format!("[{}]", config.infile_name.to_str().unwrap()).as_str());
        if config.ro {
            widget.status.push_str("[ro]");
        }

        widget
    }

    /// Returns the current position (seek) in the underlying file.
    pub fn get_seek(&mut self) -> anyhow::Result<u64>
    {
        Ok(self.file.seek(SeekFrom::Current(0))?)
    }

    /// Writes a byte at the specified offset.
    pub fn write_byte_at_offset(&mut self, byte: u8, offset: u64) -> anyhow::Result<usize>
    {
        let seek = self.get_seek()?;
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write_all(&[byte])?;

        self.file.seek(SeekFrom::Start(seek))?;

        Ok(1)
    }

    /// Writes a byte at the specified [x, y] coordinates.
    pub fn write_byte_at_position(&mut self, byte: u8, pos_y: i32, pos_x: i32) -> anyhow::Result<usize>
    {
        let offset = match self.get_seek() {
            Err(_) => return Ok(0),
            Ok(v) => v
        };

        // The offset of the current byte (under the cursor).
        let byte_offset = offset + (pos_y as u64 * 16) + (pos_x as u64);

        self.write_byte_at_offset(byte, byte_offset)
    }

    /// Writes a byte at the position of the cursor.
    pub fn write_byte_at_cursor(&mut self, byte: u8) -> anyhow::Result<usize>
    {
        let (y, x) = self.get_pos_yx();
        self.write_byte_at_position(byte, y, x)
    }

    /// Jumps to a position in the file, aligned on 16-byte positions.
    /// The cursor is advanced to its correct position.
    /// If the offset is negative, jumps from the end.
    pub fn seek(&mut self, offset: i64) -> anyhow::Result<u64>
    {
        let mut real_offset: u64 = offset as u64;

        // Jump from the end.
        if offset.is_negative() {
            let cur_seek = self.get_seek()?;

            let end = self.file.seek(SeekFrom::End(0))?;
            real_offset = (end as i64 + offset) as u64;

            self.file.seek(SeekFrom::Start(cur_seek))?;
        }

        let remainder = real_offset % 16;
        real_offset -= remainder;

        // Jump to the real offset and update the cursor position
        self.jump_to(real_offset)?;
        self.position_y = 0;
        self.position_x = remainder as i32;

        Ok(real_offset)
    }

    /// Scrolls down or up by count. returns the new seek or error.
    pub fn scroll(&mut self, direction: Direction, count: u32) -> anyhow::Result<u64>
    {
        let cur_seek = self.get_seek()?;

        // Scrolling - jumping 16 bytes up or down.
        let real_count = count * 16;

        match direction {
            Direction::Down => {
                Ok(self.jump_to(cur_seek + real_count as u64)?)
            },
            Direction::Up => {
                if (cur_seek as i64 - real_count as i64) < 0 {
                    Err(anyhow!("attempting to scroll up past beginning of the file"))
                } else {
                    Ok(self.jump_to(cur_seek - real_count as u64)?)
                }
            },
            Direction::Left => Err(anyhow!("cannot scroll left")),
            Direction::Right => Err(anyhow!("cannot scroll right")),
        }
    }

    /// Read to the buffer from the current seek.
    pub fn read_buf(&mut self) -> anyhow::Result<()>
    {
        let bytes_to_read = self.hex_win.get_max_y() * 16;
        self.buffer = util::freadn_to_vec(&mut self.file, bytes_to_read as usize)?;

        Ok(())
    }

    /// Fills the windows with formatted output. (based on internal variables)
    pub fn draw(&mut self) -> anyhow::Result<()>
    {
        self.offset_win.mv(0, 0);
        self.hex_win.mv(0, 0);
        self.canon_win.mv(0, 0);
        self.oh_sep_win.mv(0, 0);
        self.hc_sep_win.mv(0, 0);
        self.cs_sep_win.mv(0, 0);
        self.statusline_win.mv(0, 0);

        self.statusline_win.clear();
        self.statusline_win.printw(self.status.as_str());
        self.statusline_win.bkgd(pancurses::Attribute::Reverse);
        
        // Get the the number of lines and the current offset.
        let nlines = self.offset_win.get_max_y();
        let seek = self.get_seek()?;

        // Draw the seperators.
        for _ in 0..nlines {
            self.oh_sep_win.printw(SEP);
        }
        for _ in 0..nlines {
            self.hc_sep_win.printw(SEP);
        }
        for _ in 0..nlines {
            self.cs_sep_win.printw(SEP);
        }

        // Draw the offsets.
        for i in 0..nlines as u64 {
            self.offset_win.mvprintw(i as i32, 0, format!("{:08x}\n", seek + (i * 16)));
        }

        // Draw the hex bytes.
        for row in 0..nlines {
            for pair in 0..8 {
                if (row * 16 + pair * 2) % 16 != 0 {
                    self.hex_win.printw(" ");
                }

                // Check if the first byte is out of bounds.
                if row * 16 + pair * 2 >= self.buffer.len() as i32 {
                    self.hex_win.printw("  ");
                } else {
                    self.hex_win.printw(
                        format!("{:02x}", self.buffer[(row * 16 + pair * 2) as usize])
                    );
                }

                // Check if the second byte is out of bounds.
                if row * 16 + pair * 2 + 1 >= self.buffer.len() as i32 {
                    self.hex_win.printw("  ");
                } else {
                    self.hex_win.printw(
                        format!("{:02x}", self.buffer[(row * 16 + pair * 2 + 1) as usize])
                    );
                }
            }
        }

        // Draw the canonical view.
        for row in 0..nlines {
            for byte in 0..16 {
                let cur_byte;

                // Check if the character is out of bounds.
                if row * 16 + byte >= self.buffer.len() as i32 {
                    cur_byte = b' ';
                } else {
                    cur_byte = self.buffer[(row * 16 + byte) as usize];
                }

                let character = if util::check_printable(cur_byte) {
                    cur_byte as char
                } else {
                    '.'
                };
                self.canon_win.addch(character);
            }
        }

        Ok(())
    }

    /// Refresh the window and all the subwindows.
    pub fn refresh(&self)
    {
        self.win.refresh();
        self.offset_win.refresh();
        self.hex_win.refresh();
        self.canon_win.refresh();
        self.statusline_win.refresh();
        self.oh_sep_win.refresh();
        self.hc_sep_win.refresh();
        self.cs_sep_win.refresh();
    }

    /// Move the cursor. (automatically decides which pane)
    pub fn move_cursor(&mut self, direction: Direction, count: i32) -> anyhow::Result<u64>
    {
        let (orig_y, orig_x) = (self.position_y, self.position_x);
        // Get the current real position of the cursor. (relative to the pane)
        let (hex_orig_y, hex_orig_x) = self.hex_pos_to_cur(orig_y, orig_x);

        self.hex_win.mvchgat(hex_orig_y, hex_orig_x, 2, pancurses::A_NORMAL, 0);
        self.canon_win.mvchgat(orig_y, orig_x, 1, pancurses::A_NORMAL, 0);

        for _ in 0..(count) {
            self.move_cursor_once(&direction)?;
        }

        let (y, x) = (self.position_y, self.position_x);
        let (hex_y, hex_x) = self.hex_pos_to_cur(y, x);

        self.hex_win.mvchgat(hex_y, hex_x, 2, pancurses::A_BOLD, 0);
        self.canon_win.mvchgat(y, x, 1, pancurses::A_BOLD, 0);

        Ok(0)
    }

    /// Returns the absolute coordinates of the cursor. (based on the grid position)
    pub fn get_cur_yx(&self) -> (i32, i32)
    {
        match self.active_pane {
            HexPane::Hex => {
                let (y, x) = self.hex_pos_to_cur(self.position_y, self.position_x);
                (self.hex_win.get_beg_y() + y, self.hex_win.get_beg_x() + x)
            },
            HexPane::Canon => {
                (self.position_y + self.canon_win.get_beg_y(), self.position_x + self.canon_win.get_beg_x())
            }
        }
    }

    /// Returns the grid position of the cursor.
    pub fn get_pos_yx(&self) -> (i32, i32)
    {
        (self.position_y, self.position_x)
    }

    /// Switches the active pane.
    pub fn switch_pane(&mut self) -> anyhow::Result<()>
    {
        match self.active_pane {
            HexPane::Hex => {
                self.active_pane = HexPane::Canon;
            },
            HexPane::Canon => {
                self.active_pane = HexPane::Hex;
            }
        }

        Ok(self.draw()?)
    }

    /// Returns the window position of the cursor, based on the grid (virtual) position.
    pub fn hex_pos_to_cur(&self, y: i32, x:i32) -> (i32, i32)
    {
        let ret_y = y;
        // Count the character position in the hex view. (with on space between byte pairs (like xxd))
        let ret_x = (x * 2) + (x / 2);

        (ret_y, ret_x)
    }

    /// Jumps to an offset in the file and reads it into the buffer.
    fn jump_to(&mut self, offset: u64) -> anyhow::Result<u64>
    {
        let cur_seek = self.get_seek()?;
        let end = self.file.seek(SeekFrom::End(0))?;

        if offset > end {
            self.file.seek(SeekFrom::Start(cur_seek))?;
            bail!("attempting to jump beyond the end of the file");
        }

        let seek = self.file.seek(SeekFrom::Start(offset))?;
        self.read_buf()?;
        self.draw()?;

        Ok(seek)
    }

    /// Moves the cursor in the hex view.
    fn move_cursor_once(&mut self, direction: &Direction) -> anyhow::Result<u64>
    {
        let seek = self.get_seek()?;

        // Match the directions and determine if the view needs to be scrolled based on the new
        // cursor position.
        match direction {
            Direction::Up => {
                if self.position_y == 0 {
                    self.scroll(Direction::Up, 1)
                } else {
                    self.position_y -= 1;
                    Ok(seek)
                }
            },
            Direction::Down => {
                if self.position_y + 1 == self.hex_win.get_max_y() {
                    self.scroll(Direction::Down, 1)
                } else {
                    self.position_y += 1;
                    Ok(seek)
                }
            },
            Direction::Left => {
                if self.position_x == 0 {
                    if self.position_y == 0 {
                        match self.scroll(Direction::Up, 1) {
                            Err(e) => Err(e),
                            Ok(o) => {
                                self.position_x = 16 - 1;
                                Ok(o)
                            }
                        }
                    } else {
                        self.position_x = 16 -1;
                        self.position_y -= 1;
                        Ok(seek)
                    }
                } else {
                    self.position_x -= 1;
                    Ok(seek)
                }
            },
            Direction::Right => {
                if self.position_x == 16 - 1 {
                    if self.position_y + 1 == self.hex_win.get_max_y() {
                        match self.scroll(Direction::Down, 1) {
                            Err(e) => Err(e),
                            Ok(o) => {
                                self.position_x = 0;
                                Ok(o)
                            }
                        }
                    } else {
                        self.position_x = 0;
                        self.position_y += 1;
                        Ok(seek)
                    }
                } else {
                    self.position_x += 1;
                    Ok(seek)
                }
            }
        }
    }
}
