//! *Hunt the Wumpus* reimplementation in Rust.

use rand::prelude::*;
use std::error::Error;
use std::io::{self, Write};
use std::process::exit;

/// Help message.
const HELP: &str = "\
Welcome to \"Hunt the Wumpus\"

The wumpus lives in a cave of 20 rooms. Each room has 3
tunnels to other rooms. (The tunnels form a dodecahedron:
http://en.wikipedia.org/dodecahedron)

Hazards:

 Bottomless pits: Two rooms have bottomless pits in them. If you go
   there, you fall into the pit (& lose)!

 Super bats: Two other rooms have super bats. If you go
   there, a bat grabs you and takes you to some other room
   at random (which may be troublesome).

Wumpus:

   The wumpus is not bothered by hazards. (He has sucker
   feet and is too big for a bat to lift.)  Usually he is
   asleep. Two things wake him up: your shooting an arrow,
   or your entering his room.  If the wumpus wakes, he moves
   one room or stays still.  After that, if he is where you
   are, he eats you up and you lose!

You:

   Each turn you may move or shoot a crooked arrow.

   Moving: You can move one room (through one tunnel).

   Arrows: You have 5 arrows. You lose when you run out.
      You can only shoot to nearby rooms. If the arrow hits
      the wumpus, you win.

Warnings:

   When you are one room away from a wumpus or hazard, the
   computer says:

   Wumpus:  \"You smell something terrible nearby.\"
   Bat:  \"You hear a rustling.\"
   Pit:  \"You feel a cold wind blowing from a nearby cavern.\"
";

/// The maze is an dodecahedron.
const MAZE_ROOMS: usize = 20;
const ROOM_NEIGHBORS: usize = 3;

/// Number of bats.
const BATS: usize = 2;
/// Number of pits.
const PITS: usize = 2;
/// Initial number of arrows.
const ARROWS: usize = 5;

/// Fractional chance of waking the Wumpus on entry to its
/// room.
const WAKE_WUMPUS_PROB: f32 = 0.75;

/// Description of the current player state.
struct Player {
    /// Player location.
    room: usize,
    /// Remaining number of arrows.
    arrows: usize,
}

impl Player {
    /// Make a new player starting in the given room.
    fn new(room: usize) -> Self {
        Player {
            arrows: ARROWS,
            room,
        }
    }
}

/// Things that can be in a room.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RoomContents {
    Empty,
    Bat,
    Pit,
}
use RoomContents::*;

impl Default for RoomContents {
    fn default() -> Self {
        Empty
    }
}

/// Room description.
#[derive(Default, Clone, Copy)]
struct Room {
    /// The indices of neighboring rooms.
    neighbors: [usize; ROOM_NEIGHBORS],
    /// Possible danger in the room.
    contents: RoomContents,
    /// Wumpus in the room.
    wumpus: bool,
}

/// The Maze, including RNG state.
struct Maze {
    /// Room list.
    rooms: [Room; MAZE_ROOMS],
    /// RNG state.
    rng: ThreadRng,
}

impl Maze {
    // List of adjacencies used to wire up the dodecahedron.
    // https://stackoverflow.com/a/44096541/364875
    const ADJS: [[usize; 3]; 20] = [
        [1, 4, 7],
        [0, 2, 9],
        [1, 3, 11],
        [2, 4, 13],
        [0, 3, 5],
        [4, 6, 14],
        [5, 7, 16],
        [0, 6, 8],
        [7, 9, 17],
        [1, 8, 10],
        [9, 11, 18],
        [2, 10, 12],
        [11, 13, 19],
        [3, 12, 14],
        [5, 13, 15],
        [14, 16, 19],
        [6, 15, 17],
        [8, 16, 18],
        [10, 17, 19],
        [12, 15, 18],
    ];

    /// A maze is made up of rooms connected as a dodecahedron
    /// and populated according to the game rules. Return
    /// a new maze and the initial player position.
    fn new() -> (Maze, usize) {
        let mut rooms = [Room::default(); MAZE_ROOMS];

        // Place the wumpus, pits and bats in empty rooms.
        // This gets a bit involved.

        #[derive(Clone, Copy, PartialEq, Eq)]
        enum Posn {
            Wumpus,
            Player,
            Normal(RoomContents),
        };
        use Posn::*;

        let mut contents = Vec::new();
        contents.push(Wumpus);
        contents.push(Player);
        for _ in 0..PITS {
            contents.push(Normal(Pit));
        }
        for _ in 0..BATS {
            contents.push(Normal(Bat));
        }
        contents.resize(MAZE_ROOMS, Normal(Empty));
        let mut rng = rand::thread_rng();
        contents.shuffle(&mut rng);

        // Connect and populate the rooms.  This could be
        // done with zips, but is probably easier to read
        // this way.
        let mut player_loc = None;
        for (i, r) in rooms.iter_mut().enumerate() {
            r.neighbors = Maze::ADJS[i];
            match contents[i] {
                Normal(c) => r.contents = c,
                Wumpus => {
                    r.contents = Empty;
                    r.wumpus = true;
                }
                Player => {
                    r.contents = Empty;
                    player_loc = Some(i);
                }
            }
        }

        (Maze { rooms, rng }, player_loc.unwrap())
    }

    /// Description strings for adjacent dangers.
    const DESCS: &'static [(RoomContents, &'static str)] = &[
        (Pit, "You feel a cold wind blowing from a nearby cavern."),
        (Bat, "You hear a rustling."),
    ];

    /// Current room description string.
    fn describe_room(&self, room: usize) -> String {
        let mut desc_lines = Vec::new();
        desc_lines.push(format!("You are in room #{}", room));

        for (c, d) in Maze::DESCS {
            if self.is_danger_nearby(room, *c) {
                desc_lines.push(d.to_string());
            }
        }

        if let Some(_) = self.is_wumpus_nearby(room) {
            desc_lines.push(
                "You smell something terrible nearby.".to_string(),
            );
        }

        let neighbors: Vec<String> = self.rooms[room]
            .neighbors
            .iter()
            .map(|i| i.to_string())
            .collect();

        desc_lines
            .push(format!("Exits go to: {}", neighbors.join(", "),));

        desc_lines.join("\n")
    }

    /// Adjacent room contains a non-wumpus danger.
    fn is_danger_nearby(
        &self,
        room: usize,
        danger: RoomContents,
    ) -> bool {
        self.rooms[room]
            .neighbors
            .iter()
            .any(|&n| self.rooms[n].contents == danger)
    }

    /// Specific adjacent room contains the Wumpus.
    fn is_wumpus_nearby(&self, room: usize) -> Option<usize> {
        self.rooms[room]
            .neighbors
            .iter()
            .cloned()
            .find(|&n| self.rooms[n].wumpus)
    }

    /// Index of neighboring room given by user
    /// `destination`, else an error message.
    fn parse_room(
        &self,
        destination: &str,
        current_room: usize,
    ) -> Result<usize, String> {
        let dest: usize = destination.trim().parse().map_err(
            |e: std::num::ParseIntError| e.description().to_string(),
        )?;

        // Check that the given destination is the id of a linked room.
        if self.rooms[current_room]
            .neighbors
            .iter()
            .all(|&r| r != dest)
        {
            return Err("room is not a neighbor".to_string());
        }

        Ok(dest)
    }
}

/// Current game state.
enum Status {
    Normal,
    Quitting,
    Moving,
    Shooting,
}
use Status::*;

fn main() {
    let (mut maze, player_loc) = Maze::new();
    let mut status = Normal;
    let mut player = Player::new(player_loc);

    let describe = |maze: &Maze, player: &Player| {
        println!("{}", maze.describe_room(player.room));
        println!("What do you want to do? (m)ove or (s)hoot?");
    };

    let prompt = || {
        print!("> ");
        io::stdout().flush().expect("Error flushing");
    };

    describe(&maze, &player);

    loop {
        prompt();
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .expect("Cannot read from stdin");
        let input: &str = &input.trim().to_lowercase();

        match status {
            Quitting => match input {
                "y" => {
                    println!("Goodbye, braveheart!");
                    exit(0);
                }
                "n" => {
                    println!("Good. the Wumpus is looking for you!");
                    status = Normal;
                }
                _ => println!("That doesn't make any sense"),
            },
            Moving => {
                match maze.parse_room(input, player.room) {
                    Ok(room) => {
                        if maze.rooms[room].wumpus {
                            println!("The wumpus ate you up!");
                            println!("GAME OVER");
                            exit(0);
                        } else if maze.rooms[room].contents == Pit {
                            println!("You fall into a bottomless pit!");
                            println!("GAME OVER");
                            exit(0);
                        } else if maze.rooms[room].contents == Bat {
                            println!("The bats whisk you away!");
                            player.room =
                                maze.rng.gen_range(0, maze.rooms.len());
                        } else {
                            player.room = room;
                        }

                        status = Normal;
                        describe(&maze, &player);
                    }
                    Err(e) => {
                        println!("There was a problem with your directions: {}", e);
                        println!("Where do you want to go?");
                    }
                }
            }
            Shooting => {
                match maze.parse_room(input, player.room) {
                    Ok(room) => {
                        if maze.rooms[room].wumpus {
                            println!("YOU KILLED THE WUMPUS! GOOD JOB, BUDDY!!!");
                            exit(0);
                        }

                        if let Some(wumpus_room) =
                            maze.is_wumpus_nearby(room)
                        {
                            // 75% chances of waking up the wumpus that would go into another room
                            if maze.rng.gen::<f32>() <= WAKE_WUMPUS_PROB
                            {
                                let new_wumpus_room = *maze.rooms
                                    [wumpus_room]
                                    .neighbors
                                    .choose(&mut maze.rng)
                                    .unwrap();

                                if new_wumpus_room == player.room {
                                    println!("You woke up the wumpus and he ate you!");
                                    println!("GAME OVER");
                                    exit(1);
                                }

                                maze.rooms[wumpus_room].wumpus = false;
                                maze.rooms[new_wumpus_room].wumpus =
                                    true;
                                println!("You heard a rumbling in a nearby cavern.");
                            }
                        }

                        player.arrows -= 1;
                        if player.arrows == 0 {
                            println!("You ran out of arrows.");
                            println!("GAME OVER");
                            exit(1);
                        }

                        status = Normal;
                        describe(&maze, &player);
                    }
                    Err(e) => {
                        println!("There was a problem with your directions: {}", e);
                        println!("Where do you want to shoot?");
                    }
                }
            }
            Normal => match input {
                "h" => println!("{}", HELP),
                "q" => {
                    println!("Are you so easily scared? [y/n]");
                    status = Quitting;
                }
                "m" => {
                    println!("Where?");
                    status = Moving;
                }
                "s" => {
                    println!("Where?");
                    status = Shooting;
                }
                _ => println!("That doesn't make any sense"),
            },
        }
    }
}
