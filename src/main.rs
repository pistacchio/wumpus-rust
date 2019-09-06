use std::cell::{Cell, RefCell};
use std::io;
use std::io::Write;
use std::process::exit;
use std::rc::Rc;
use std::ops::{DerefMut, Deref};
use rand::prelude::ThreadRng;
use rand::prelude::*;


///////////////
// CONSTANTS //
///////////////

const HELP: &str = "\
Welcome to \"Hunt the Wumpus\"
The wumpus lives in a cave of 20 rooms. Each room has 3 tunnels to
other rooms. (Look at a dodecahedron to see how this works. If you
dont know what a dodecahedron is, ask someone.)
Hazards:
 Bottomless pits - Two rooms have bottomless pits in them. If you go
   there, you fall into the pit (& lose)!
 Super bats - Two other rooms have super bats. If you go there, a
   bat grabs you and takes you to some other room at random (which
   may be troublesome).
Wumpus:
   The wumpus is not bothered by hazards. (He has sucker feet and is
   too big for a bat to lift.)  Usually he is asleep. Two things
   wake him up: your shooting an arrow, or your entering his room.
   If the wumpus wakes, he moves one room or stays still.
   After that, if he is where you are, he eats you up and you lose!
You:
   Each turn you may move or shoot a crooked arrow.
   Moving:  You can move one room (through one tunnel).
   Arrows:  You have 5 arrows.  You lose when you run out.
      You can only shoot to nearby rooms.
      If the arrow hits the wumpus, you win.
Warnings:
   When you are one room away from a wumpus or hazard, the computer
   says:
   Wumpus:  \"You smell something terrible nearby.\"
   Bat   :  \"You hear a rustling.\"
   Pit   :  \"You feel a cold wind blowing from a nearby cavern.\"
";

const MAZE_ROOMS: usize = 20;
const ROOM_NEIGHBOURS: usize = 3;

const BATS: usize = 2;
const PITS: usize = 2;
const ARROWS: usize = 5;

const WAKE_WUMPUS_PROB: f32 = 0.75;

type RoomNum = usize;

////////////
// PLAYER //
////////////

#[derive(Debug)]
struct Player {
    room: RoomNum,
    arrows: usize,
}

impl Player {
    fn new(room: RoomNum) -> Self {
        Player {
            arrows: ARROWS,
            room,
        }
    }
}

//////////
// ROOM //
//////////

#[derive(Debug, PartialEq)]
enum Danger {
    Wumpus,
    Bat,
    Pit,
}

#[derive(Default, Debug)]
struct Room {
    id: RoomNum,
    neighbours: [Cell<Option<RoomNum>>; ROOM_NEIGHBOURS],
    dangers: Vec<Danger>,
}

impl Room {
    fn new(id: RoomNum) -> Self {
        let default_room = Room::default();

        Room {
            id,
            ..default_room
        }
    }

    fn neighbour_ids(&self) -> Vec<RoomNum> {
        self.neighbours.iter()
            .filter(|n| n.get().is_some())
            .map(|n| n.get().unwrap())
            .collect()
    }
}

//////////
// MAZE //
//////////

#[derive(Debug)]
struct Maze {
    rooms: Vec<Room>,
    rng: Rc<RefCell<ThreadRng>>,
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

    // Builds a vector of rooms comprising a dodecahedron.
    fn new(rng: Rc<RefCell<ThreadRng>>) -> Self {
        let mut rooms: Vec<Room> = (0..MAZE_ROOMS)
            .map(|idx| Room::new(idx as RoomNum))
            .collect();

        for (i, room) in rooms.iter_mut().enumerate() {
            for (j, nb) in room.neighbours.iter_mut().enumerate() {
                nb.set(Some(Maze::ADJS[i][j]));
            }
        }

        let mut maze = Maze {
            rooms,
            rng,
        };

        // place the wumpus, pits and bats in empty rooms
        let empty_room = maze.rnd_empty_room();
        maze.rooms[empty_room].dangers.push(Danger::Wumpus);

        for _ in 0..PITS {
            let empty_room = maze.rnd_empty_room();
            maze.rooms[empty_room].dangers.push(Danger::Pit);
        }

        for _ in 0..BATS {
            let empty_room = maze.rnd_empty_room();
            maze.rooms[empty_room].dangers.push(Danger::Bat);
        }

        maze
    }

    fn rnd_empty_room(&mut self) -> RoomNum {
        let empty_rooms: Vec<_> = self.rooms.iter()
            .filter(|n| n.dangers.is_empty())
            .collect();

        empty_rooms
            .choose(RefCell::borrow_mut(&self.rng).deref_mut())
            .unwrap()
            .id
    }

    fn rnd_empty_neighbour(&mut self, room: RoomNum) -> Option<RoomNum> {
        let neighbour_ids = self.rooms[room].neighbour_ids();

        let empty_neighbours: Vec<_> = neighbour_ids.iter()
            .filter(|&n| self.rooms[*n].dangers.is_empty())
            .collect();

        if empty_neighbours.is_empty() {
            return None;
        }

        let empty_neighbour = empty_neighbours
            .choose(RefCell::borrow_mut(&self.rng).deref_mut())
            .unwrap();

        Some(**empty_neighbour)
    }

    fn describe_room(&self, room: RoomNum) -> String {
        let mut description = format!("You are in room #{}", room);

        if self.is_danger_nearby(room, Danger::Pit) {
            description.push_str("\nYou feel a cold wind blowing from a nearby cavern.");
        }
        if self.is_danger_nearby(room, Danger::Bat) {
            description.push_str("\nYou hear a rustling.");
        }
        if self.is_danger_nearby(room, Danger::Wumpus) {
            description.push_str("\nYou smell something terrible nearby.");
        }

        description.push_str(&format!("\nExits go to: {}",
                                      self.rooms[room].neighbours
                                          .iter()
                                          .map(|n| n.get().unwrap().to_string())
                                          .collect::<Vec<String>>()
                                          .join(", ")));

        description
    }

    fn is_danger_nearby(&self, room: RoomNum, danger: Danger) -> bool {
        self.rooms[room].neighbours.iter().find(|n| {
            self.rooms[n.get().unwrap()]
                .dangers.contains(&danger)
        }).is_some()
    }

    fn parse_room(&self, destination: &str, current_room: RoomNum) -> Result<RoomNum, ()> {
        let destination: Result<RoomNum, _> = destination.parse();

        // check that the given destination is both a number an the number of a linked room
        if let Ok(room) = destination {
            if self.rooms[current_room].neighbour_ids().contains(&room) {
                return Ok(room);
            }
        }

        Err(())
    }
}

#[test]
fn test_maze_connected() {
    use std::collections::HashSet;
    let rng = Rc::new(RefCell::new(rand::thread_rng()));
    let maze = Maze::new(rng.clone());
    let n = maze.rooms.len();

    fn exists_path(
        i: RoomNum,
        j: RoomNum,
        vis: &mut HashSet<RoomNum>,
        maze: &Maze)
        -> bool
    {
        if i == j {
            return true;
        }
        vis.insert(i);
        maze.rooms[i].neighbours.iter().any(|neighbour| {
            // Check that all rooms have three neighbors.
            let k = neighbour.get().unwrap();
            !vis.contains(&k) && exists_path(k, j, vis, maze)
        })
    }
    for i in 0..n {
        for j in 0..n {
            assert!(exists_path(i, j, &mut HashSet::new(), &maze));
        }
    }
}

///////////////
// MAIN LOOP //
///////////////

enum Status {
    Normal,
    Quitting,
    Moving,
    Shooting,
}

fn main() {
    let rng = Rc::new(RefCell::new(rand::thread_rng()));
    let mut maze = Maze::new(rng.clone());
    let mut player = Player::new(maze.rnd_empty_room());
    let mut status = Status::Normal;

    let describe = |maze: &Maze, player: &Player| {
        println!("{}", maze.describe_room(player.room));
        println!("What do you want to do? (m)ove or (s)hoot?");
    };

    let prompt = || {
        print!("> ");
        io::stdout().flush().expect("Error flushing");
    };

    describe(&maze, &player);
    prompt();

    // main loop
    loop {
        let mut input = String::new();
        io::stdin().read_line(&mut input).expect("Cannot read from stdin");
        let input: &str = &input.trim().to_lowercase();

        match status {
            Status::Quitting => {
                match input {
                    "y" => {
                        println!("Goodbye, braveheart!");
                        exit(0);
                    }
                    "n" => {
                        println!("Good. the Wumpus is looking for you!");
                        status = Status::Normal;
                    }
                    _ => println!("That doesn't make any sense")
                }
            }
            Status::Moving => {
                if let Ok(room) = maze.parse_room(input, player.room) {
                    if maze.rooms[room].dangers.contains(&Danger::Wumpus) {
                        println!("The wumpus ate you up!\nGAME OVER");
                        exit(0);
                    } else if maze.rooms[room].dangers.contains(&Danger::Pit) {
                        println!("You fall into a bottomless pit!\nGAME OVER");
                        exit(0);
                    } else if maze.rooms[room].dangers.contains(&Danger::Bat) {
                        println!("The bats whisk you away!");
                        player.room = maze.rnd_empty_room();
                    } else {
                        player.room = room;
                    }

                    status = Status::Normal;
                    describe(&maze, &player);
                } else {
                    println!("There are no tunnels from here to that room. Where do you wanto do go?");
                }
            }
            Status::Shooting => {
                if let Ok(room) = maze.parse_room(input, player.room) {
                    if maze.rooms[room].dangers.contains(&Danger::Wumpus) {
                        println!("YOU KILLED THE WUMPUS! GOOD JOB, BUDDY!!!");
                        exit(0);
                    } else {
                        // 75% chances of waking up the wumpus that would go into another room
                        if RefCell::borrow_mut(rng.deref()).gen::<f32>() < WAKE_WUMPUS_PROB {
                            let wumpus_room = maze.rooms.iter()
                                .find(|r| r.dangers.contains(&Danger::Wumpus))
                                .unwrap()
                                .id;

                            if let Some(new_wumpus_room) = maze.rnd_empty_neighbour(wumpus_room) {
                                if new_wumpus_room == player.room {
                                    println!("You woke up the wumpus and he ate you!\nGAME OVER");
                                    exit(1);
                                }

                                maze.rooms[wumpus_room].dangers.retain(|d| d != &Danger::Wumpus);
                                maze.rooms[new_wumpus_room].dangers.push(Danger::Wumpus);
                                println!("You heard a rumbling in a nearby cavern.");
                            }
                        }

                        player.arrows -= 1;
                        if  player.arrows == 0 {
                            println!("You ran out of arrows.\nGAME OVER");
                            exit(1);
                        }

                        status = Status::Normal;
                    }
                } else {
                    println!("There are no tunnels from here to that room. Where do you wanto do shoot?");
                }
            }
            _ => {
                match input {
                    "h" => println!("{}", HELP),
                    "q" => {
                        println!("Are you so easily scared? [y/n]");
                        status = Status::Quitting;
                    }
                    "m" => {
                        println!("Where?");
                        status = Status::Moving;
                    }
                    "s" => {
                        println!("Where?");
                        status = Status::Shooting;
                    }
                    _ => println!("That doesn't make any sense")
                }
            }
        }

        prompt();
    }
}
