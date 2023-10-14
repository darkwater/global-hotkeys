mod config;

use anyhow::{anyhow, Result};
use clap::Parser;
use input::event::keyboard::KeyboardEventTrait;
use input::event::tablet_pad::KeyState;
use input::event::KeyboardEvent;
use input::{Event, Libinput, LibinputInterface};
use itertools::Itertools;
use libc::{O_RDWR, O_WRONLY};
use raw_tty::GuardMode;
use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io::stdin;
use std::iter;
use std::os::unix::{fs::OpenOptionsExt, io::OwnedFd};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::config::ParsedHotkey;

struct Interface;

impl LibinputInterface for Interface {
    fn open_restricted(&mut self, path: &Path, flags: i32) -> Result<OwnedFd, i32> {
        OpenOptions::new()
            .custom_flags(flags)
            .read(/*(flags & O_RDONLY != 0) |*/ flags & O_RDWR != 0)
            .write((flags & O_WRONLY != 0) | (flags & O_RDWR != 0))
            .open(path)
            .map(|file| file.into())
            .map_err(|err| err.raw_os_error().unwrap())
    }

    fn close_restricted(&mut self, fd: OwnedFd) {
        drop(File::from(fd));
    }
}

#[derive(Parser)]
struct Cli {
    /// Show keycodes of pressed keys
    #[clap(short, long)]
    read: bool,
}

fn main() -> Result<()> {
    let args = Cli::parse();

    let mut input = Libinput::new_with_udev(Interface);
    input
        .udev_assign_seat("seat0")
        .map_err(|()| anyhow!("failed to assign seat"))?;

    if args.read {
        println!("Press the same key 3 times to exit");
        println!("Warning: inputs may be buffered and sent to your shell after exit.");
        println!(
            "Don't enter anything that your shell is gonna run. Maybe don't press enter at all."
        );

        let mut stdin = stdin().guard_mode()?;
        stdin.set_raw_mode()?;

        // let running = Arc::new(AtomicBool::new(true));

        // thread::spawn({
        //     let running = running.clone();
        //     move || {
        //         let mut buf = [0; 32];
        //         let n = stdin.read(&mut buf).unwrap();
        //         if n == 0 {
        //             running.store(false, Ordering::Relaxed);
        //         }
        //     }
        // });

        let mut exit_counter = 0u32;
        let mut exit_keycode = 0;

        'outer: loop {
            // while running.load(Ordering::Relaxed) {
            input.dispatch()?;
            for event in &mut input {
                if let Event::Keyboard(KeyboardEvent::Key(key)) = event {
                    print!("{:?} {:?}\r\n", key.key_state(), key.key());

                    if key.key_state() == KeyState::Released {
                        if key.key() == exit_keycode {
                            exit_counter += 1;
                        } else {
                            exit_counter = 1;
                            exit_keycode = key.key();
                        }

                        if exit_counter == 3 {
                            break 'outer;
                        }
                    }
                }
            }
        }

        return Ok(());
    }

    let config = config::load()?;

    dbg!(&config);

    let hotkeys = config
        .hotkeys
        .iter()
        .map(|hotkey| ParsedHotkey::new(hotkey, &config.keycodes))
        .collect::<Result<Vec<_>>>()?;

    let mut held_down = BTreeSet::new();
    let mut children = Vec::new();

    loop {
        input.dispatch()?;
        for event in &mut input {
            if let Event::Keyboard(KeyboardEvent::Key(key)) = event {
                print!("{:?} {:?}\r\n", key.key_state(), key.key());

                match key.key_state() {
                    KeyState::Pressed => {
                        let keys = held_down
                            .iter() // sorted because btree
                            .copied()
                            .chain(iter::once(key.key()))
                            .collect::<Vec<_>>();

                        for hotkey in &hotkeys {
                            if hotkey.keys == keys {
                                println!("Matched hotkey: {:?}", hotkey);

                                let envkeys = config.env.keys().join(",");

                                let child = Command::new("sudo")
                                    .args(["-u", &config.run_as])
                                    .args([format!("--preserve-env={envkeys}")])
                                    .args(["sh", "-c", &hotkey.command])
                                    .envs(&config.env)
                                    .spawn()?;

                                children.push(child);
                            }
                        }

                        held_down.insert(key.key());
                    }
                    KeyState::Released => {
                        held_down.retain(|&keycode| keycode != key.key());
                    }
                }
            }
        }

        children.retain_mut(|child| {
            if let Some(status) = child.try_wait().unwrap() {
                println!("Child exited with status: {}", status);
                false
            } else {
                true
            }
        });

        std::thread::sleep(Duration::from_millis(5));
    }
}
