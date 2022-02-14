#![allow(dead_code)]
use std::fs::{OpenOptions, File};
use pancurses::Input;

mod editor;
mod options;
mod util;
mod widget;

use editor::Editor;
use widget::Direction;

fn main()
{
    let argv: Vec<String> = std::env::args().collect();

    // Parse the options. (quitting if needed)

    let mut options = match options::parse_options() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{}: {}", argv[0], &e);
            options::usage();
            std::process::exit(1);
        }
    };
    
    // Error check.

    if !options.has_infile {
        eprintln!("{}: no file specified", argv[0]);
        options::usage();
        std::process::exit(1);
    }

    // Attempt to open the file as rw. If failed, attempt to open it as ro. Else, exit.

    let infile = match OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&options.infile_name)
    {
        Ok(f) => f,
        Err(_) => match File::open(&options.infile_name) {
            Ok(f) => {
                options.ro = true;
                f
            },
            Err(e) => {
                eprintln!("{}: {}: {}", argv[0], options.infile_name.to_str().unwrap(), &e);
                std::process::exit(1);
            },
        },
    };

    // Initialise the editor.
    let mut editor = Editor::init(infile, options);

    // Loop keyboard events.
    loop {
        match editor.getch() {
            Some(Input::Character(c)) => {
                if c == 'q' {
                    break;
                } else if c == 'h' {
                    editor.move_cursor(Direction::Left, 1);
                } else if c == 'j' {
                    editor.move_cursor(Direction::Down, 1);
                } else if c == 'k' {
                    editor.move_cursor(Direction::Up, 1);
                } else if c == 'l' {
                    editor.move_cursor(Direction::Right, 1);
                } else if c == 'g' {
                    editor.seek(0);
                } else if c == 'G' {
                    editor.seek(-1);
                } else if c == 'u' {
                    editor.scroll(Direction::Up, 1);
                } else if c == 'd' {
                    editor.scroll(Direction::Down, 1);
                } else if c == '\t' {
                    editor.switch_pane();
                } else if c == 't' {
                    editor.switch_view();
                } else if c == 'r' {
                    editor.replace().ok();
                } else if c == 'R' {
                    editor.replace_many().ok();
                //} else if c == ':' {
                    //editor.command();
                }
            },
            Some(Input::KeyRight) => {
                editor.move_cursor(Direction::Right, 1);
            },
            Some(Input::KeyLeft) => {
                editor.move_cursor(Direction::Left, 1);
            },
            Some(Input::KeyUp) => {
                editor.move_cursor(Direction::Up, 1);
            },
            Some(Input::KeyDown) => {
                editor.move_cursor(Direction::Down, 1);
            },
            Some(Input::KeyHome) => {
                editor.seek(0);
            },
            Some(Input::KeyEnd) => {
                editor.seek(-1);
            },
            Some(Input::KeyResize) => {
                pancurses::resize_term(0, 0);
            }
            Some(_) => (),
            None => (),
        }
        editor.refresh();
    }

    editor.end()
}
