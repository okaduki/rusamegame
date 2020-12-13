extern crate termion;
extern crate rand;
extern crate clap;

use std::{fmt::{self, Display}, io::{stdin, stdout, Read, Write}};
use std::collections::HashSet;
use std::process::exit;


use clap::{Arg, App};

use rand::prelude::*;

use termion::{color, style, cursor};
use termion::event::Key;
use termion::input::{TermRead, Keys};
use termion::raw::IntoRawMode;
use termion::screen::AlternateScreen;

const HELP_MSG: &str = r#"
controls:
    ---selection--------------------
    space | enter ~ delete the current cell.
    ---movement---------------------
    h ~ move left.
    j ~ move down.
    k ~ move up.
    l ~ move right.
    ---control----------------------
    q     ~ quit game.
    r     ~ restart game.
"#;


struct ColorCompl {
    fr: color::Rgb,
    bg: color::Rgb,
}

impl ColorCompl {
    fn new(
        fr: u8,
        fg: u8,
        fb: u8
    ) -> Self {
        let mx = *[fr, fg, fb].iter().max().unwrap() as u16;
        let mn = *[fr, fg, fb].iter().min().unwrap() as u16;
        let sum = mx + mn;
        ColorCompl {
            fr: color::Rgb(fr, fg, fb),
            bg: color::Rgb((sum - fr as u16) as u8, (sum - fg as u16) as u8, (sum - fb as u16) as u8),
        }
    }

    fn fr_string(&self) -> String { self.fr.fg_string() }
    fn bg_string(&self) -> String { self.bg.bg_string() }
}

#[derive(Clone, Copy)]
struct Cell<'a> {
    kind: u8,
    empty: bool,
    color_table: &'a [ColorCompl],
}

impl<'a> Cell<'a> {
    fn fr_string(&self) -> String {
        let color = &self.color_table[self.kind as usize];
        color.fr_string()
    }

    fn bg_string(&self) -> String {
        let color = &self.color_table[self.kind as usize];
        color.bg_string()
    }
}

impl<'a> Display for Cell<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let c = if self.empty {
                ' '
            }
            else {
                (self.kind + 'A' as u8) as char
            };
        write!(f, "{}{}{}", self.fr_string(), c, style::Reset)
    }
}


struct Game<'a, R: Read, W: Write> {
    width: u16,
    height: u16,
    grid: Box<[Cell<'a>]>,
    x: u16,
    y: u16,
    difficulty: u16,
    score: u32,
    output: W,
    input: Keys<R>,
    rng: rand::rngs::ThreadRng,
}

fn init <R: TermRead + Read, W: Write>(stdout: W, stdin: R, difficulty: u16, width: u16, height: u16, colors: &[ColorCompl]) {
    let rng = rand::thread_rng();
    let mut game = Game {
        width,
        height,
        grid: vec![
            Cell {
                kind: 0u8,
                empty: false,
                color_table: colors,
            }; (width * height) as usize]
            .into_boxed_slice(),
        x: 0,
        y: 0,
        difficulty,
        score: 0,
        output: stdout,
        input: stdin.keys(),
        rng
    };

    game.reset();
    game.start();
}

// impl<R: Iterator<Item=Result<Key, std::io::Error>>, W: Write> Game<R, W> {
impl<'a, R: Read, W: Write> Game<'a, R, W> {
    fn moveto(&mut self, x: u16, y: u16) {
        write!(self.output, "{}", cursor::Goto(x + 1, y + 1)).unwrap();
    }
    
    fn pos(&self, x: u16, y: u16) -> usize {
        (y * self.width + x) as usize
    }

    fn get_mut(&mut self, x: u16, y: u16) -> &mut Cell<'a> {
        &mut self.grid[self.pos(x, y)]
    }

    fn reset(&mut self) {
        write!(self.output, "{}", termion::clear::All).unwrap();

        self.x = 0;
        self.y = 0;
        self.score = 0;
        for iu16 in 0 .. self.width * self.height {
            let i = iu16 as usize;
            let mut cell = &mut self.grid[i];
            let k = (self.rng.gen::<u16>() % self.difficulty) as u8;
            cell.kind = k;
            cell.empty = false;
        };

        self.refresh();
        self.moveto(0, 0);
        self.output.flush().unwrap();
    }

    fn get_connected_aux(&self, x: u16, y: u16, k: u8, res: &mut Vec<(u16,u16)>,
        visited: &mut HashSet<(u16,u16)>)
    {
        visited.insert((x, y));
        res.push((x, y));

        let mut candidate = vec![];

        if x > 0 {
            let (tx, ty) = (x - 1, y);
            candidate.push((tx, ty));
        }
        if x + 1 < self.width {
            let (tx, ty) = (x + 1, y);
            candidate.push((tx, ty));
        }
        if y > 0 {
            let (tx, ty) = (x, y - 1);
            candidate.push((tx, ty));
        }
        if y + 1 < self.height {
            let (tx, ty) = (x, y + 1);
            candidate.push((tx, ty));
        }
        
        for (tx, ty) in candidate {
            if visited.get(&(tx, ty)).is_none()
                && !self.grid[self.pos(tx, ty)].empty
                && self.grid[self.pos(tx, ty)].kind == k {
                self.get_connected_aux(tx, ty, k, res, visited);
            }
        }
    }

    fn get_connected_pos(&self, x: u16, y: u16) -> Vec<(u16,u16)> {
        let mut res = Vec::new();
        let mut visited = HashSet::new();
        self.get_connected_aux(x, y,
            self.grid[self.pos(x, y)].kind, &mut res, &mut visited);
        res
    }

    fn get_connected(&self) -> Vec<(u16,u16)> {
        self.get_connected_pos(self.x, self.y)
    }

    fn make_fore(&mut self) {
        if self.grid[self.pos(self.x, self.y)].empty { return }
        let cell_pos = self.get_connected();

        for (x, y) in cell_pos {
            self.moveto(x, y);
            let c = self.grid[self.pos(x, y)];
            write!(self.output, "{}{}", c.bg_string(), c).unwrap();
        }
    }

    fn make_back(&mut self) {
        if self.grid[self.pos(self.x, self.y)].empty { return }
        let cell_pos = self.get_connected();

        for (x, y) in cell_pos {
            self.moveto(x, y);
            let c = self.grid[self.pos(x, y)];
            write!(self.output, "{}", c).unwrap();
        }
    }

    fn calc_score(&self, num: u32) -> u32 {
        if num < 2 { 0 }
        else { (num - 2) * (num - 2) }
    }

    fn refresh(&mut self) {
        for y in 0..self.height {
            for x in 0..self.width {
                let cell = self.grid[self.pos(x, y)];
                self.moveto(x, y);
                write!(self.output, "{}", cell).unwrap();
            }
        }
    }

    fn delete(&mut self) {
        if self.grid[self.pos(self.x, self.y)].empty { return }
        let cell_pos = self.get_connected();
        if cell_pos.len() <= 1 { return }

        self.score += self.calc_score(cell_pos.len() as u32);

        for (x, y) in cell_pos {
            self.get_mut(x, y).empty = true;
            self.moveto(x, y);
            write!(self.output, "{}", self.grid[self.pos(x, y)]).unwrap();
        }

        self.update();
    }

    fn is_gameover(&mut self) -> bool {
        for y in 0..self.height {
            for x in 0..self.width {
                if self.grid[self.pos(x, y)].empty { continue; }
                let cell_pos = self.get_connected_pos(x, y);
                if cell_pos.len() > 1 { return false; }
            }
        }
        true
    }

    fn update(&mut self) {
        for x in 0..self.width {
            let mut bottom_y: i16 = (self.height - 1) as i16;
            for y in (0..self.height).rev() {
                let p = self.pos(x, y);
                if !self.grid[p].empty {
                    let bp = self.pos(x, bottom_y as u16);
                    self.grid[bp] = self.grid[p];
                    bottom_y -= 1;
                }
            }

            for y in 0..=bottom_y {
                self.grid[self.pos(x, y as u16)].empty = true;
            }
        }

        let mut left_x = 0;
        for x in 0..self.width {
            if !self.grid[self.pos(x, self.height - 1)].empty {
                if left_x < x {
                    for y in 0..self.height {
                        self.grid[self.pos(left_x, y)] = self.grid[self.pos(x, y)];
                    }
                }
                left_x += 1;
            }
        }
        
        for x in left_x..self.width {
            for y in 0..self.height {
                self.grid[self.pos(x, y)].empty = true;
            }
        }

        self.refresh();
    }

    fn print_score(&mut self) {
        self.moveto(0, self.height + 2);
        write!(self.output, "score: {}", self.score).unwrap();
    }

    fn print_message(&mut self) {
        if self.is_gameover() {
            self.moveto(15, self.height + 2);
            write!(self.output, "gameover; 'r': restart, 'q': quit").unwrap();
        }
    }

    fn start(&mut self) {
        write!(self.output, "{}", cursor::Save).unwrap();
        write!(self.output, "{}{}", cursor::Show, cursor::SteadyBar).unwrap();
        self.make_fore();
        self.print_score();
        self.moveto(self.x, self.y);
        self.output.flush().unwrap();

        loop {
            let key = self.input.next().unwrap().unwrap();

            if key == Key::Ctrl('c') {
                break;
            }

            self.make_back();
            if let Key::Char(c) = key {
                match c {
                    'h' if self.x > 0 => { self.x -= 1; },
                    'j' if self.y + 1 < self.height => { self.y += 1; },
                    'k' if self.y > 0 => { self.y -= 1; },
                    'l' if self.x + 1 < self.width => { self.x += 1; },
                    ' ' | '\n' => { self.delete(); },
                    'q' => { break; }
                    'r' => { self.reset(); }
                    _ => {}
                }
            }

            self.make_fore();
            self.print_score();
            self.print_message();
            self.moveto(self.x, self.y);
            self.output.flush().unwrap();
        }

        write!(self.output, "{}", cursor::Restore).unwrap();
        self.output.flush().unwrap();
    }
}

fn main() {
    let matches = App::new("rusamegame")
        .version("1.0")
        .author("okaduki <okaduki1@gmail.com>")
        .about(HELP_MSG)
        .arg(Arg::with_name("row")
            .short("r")
            .long("row")
            .value_name("row")
            .default_value("16")
            .help("grid size")
            .takes_value(true))
        .arg(Arg::with_name("col")
            .short("c")
            .long("col")
            .value_name("col")
            .default_value("16")
            .help("grid size")
            .takes_value(true))
        .arg(Arg::with_name("difficulty")
            .short("d")
            .long("diff")
            .value_name("diff")
            .default_value("3")
            .help("difficulty (1 - 6)")
            .takes_value(true))
        .get_matches();
    
    let colors: [ColorCompl; 6] = [
        ColorCompl::new(216, 38, 38),
        ColorCompl::new(38, 216, 38),
        ColorCompl::new(38, 38, 216),
        ColorCompl::new(216, 216, 38),
        ColorCompl::new(38, 216, 216),
        ColorCompl::new(216, 38, 216),
    ];

    let width = matches.value_of("col").unwrap_or("16").parse()
        .unwrap_or_else(|_| { eprintln!("col is not a number"); exit(1) });
    let height = matches.value_of("row").unwrap_or("16").parse()
        .unwrap_or_else(|_| { eprintln!("row is not a number"); exit(1) });
    let difficulty = matches.value_of("difficulty").unwrap_or("3").parse()
        .unwrap_or_else(|_| { eprintln!("difficulty is not a number"); exit(1) });

    if difficulty > colors.len() as u16 {
        eprintln!("difficulty should be in 1 - 6, but got {}", difficulty);
        exit(1);
    }

    let stdin = stdin();
    let stdout = stdout().into_raw_mode().unwrap();
    let stdout = AlternateScreen::from(stdout);
    init(stdout, stdin, difficulty, width, height, &colors);
}