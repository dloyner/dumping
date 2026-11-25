use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::env;
use std::hash::{BuildHasherDefault, Hasher};
use std::time::Instant;

const ALPHABET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIRECTIONS: [char; 4] = ['U', 'D', 'R', 'L'];

const BITS: u32 = 5;
const CELL_MASK: u128 = 0b11111;
const MAX_N: usize = 5;
const NO_PARENT: u32 = u32::MAX;

type State = u128;

#[derive(Default, Clone, Copy)]
struct FxHasher(u64);

const FX_K: u64 = 0x517c_c1b7_2722_0a95;

impl FxHasher {
    #[inline(always)]
    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(FX_K);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut buf = [0u8; 8];
            buf[..chunk.len()].copy_from_slice(chunk);
            self.add(u64::from_le_bytes(buf));
        }
    }

    #[inline(always)]
    fn write_u128(&mut self, i: u128) {
        self.add(i as u64);
        self.add((i >> 64) as u64);
    }

    #[inline(always)]
    fn finish(&self) -> u64 {
        self.0.rotate_left(26)
    }
}

type FxBuild = BuildHasherDefault<FxHasher>;

fn inverse(direction: char) -> char {
    match direction {
        'U' => 'D',
        'D' => 'U',
        'L' => 'R',
        'R' => 'L',
        _ => unreachable!(),
    }
}

struct Move {
    name: String,
    inverse_name: String,
    clear: State,
    src: [u32; MAX_N],
    dst: [u32; MAX_N],
    len: usize,
}

fn build_moves(n: usize) -> Vec<Move> {
    let mut moves = Vec::with_capacity(n * 4);
    for idx in 0..n {
        let col: Vec<usize> = (0..n).map(|r| r * n + idx).collect();
        let row: Vec<usize> = (0..n).map(|c| idx * n + c).collect();
        for d in DIRECTIONS {
            let line = if d == 'U' || d == 'D' { &col } else { &row };
            let shift = if d == 'U' || d == 'L' { 1 } else { n - 1 };

            let mut mv = Move {
                name: format!("{idx}{d}"),
                inverse_name: format!("{idx}{}", inverse(d)),
                clear: 0,
                src: [0; MAX_N],
                dst: [0; MAX_N],
                len: n,
            };
            for (k, &pos) in line.iter().enumerate() {
                let dst = pos as u32 * BITS;
                mv.dst[k] = dst;
                mv.src[k] = line[(k + shift) % n] as u32 * BITS;
                mv.clear |= CELL_MASK << dst;
            }
            moves.push(mv);
        }
    }
    moves
}

#[inline(always)]
fn apply(state: State, mv: &Move) -> State {
    let mut out = state & !mv.clear;
    for k in 0..mv.len {
        out |= ((state >> mv.src[k]) & CELL_MASK) << mv.dst[k];
    }
    out
}

fn pack(cells: &[u8]) -> State {
    cells
        .iter()
        .enumerate()
        .fold(0, |acc, (i, &b)| acc | (((b - b'A') as State) << (i as u32 * BITS)))
}

fn unpack(state: State, cells: usize) -> String {
    (0..cells)
        .map(|i| (b'A' + ((state >> (i as u32 * BITS)) & CELL_MASK) as u8) as char)
        .collect()
}

struct Node {
    state: State,
    parent: u32,
    mv: u32,
}

struct Search {
    nodes: Vec<Node>,
    index: HashMap<State, u32, FxBuild>,
    level: Vec<u32>,
}

impl Search {
    fn new(root: State) -> Self {
        let mut index = HashMap::with_capacity_and_hasher(1 << 16, FxBuild::default());
        index.insert(root, 0);
        Search {
            nodes: vec![Node {
                state: root,
                parent: NO_PARENT,
                mv: 0,
            }],
            index,
            level: vec![0],
        }
    }

    fn expand(&mut self, other: &Search, moves: &[Move]) -> Option<State> {
        let mut next_level = Vec::with_capacity(self.level.len() * moves.len() / 2);

        for &cur in &self.level {
            let cur_state = self.nodes[cur as usize].state;
            for (mi, mv) in moves.iter().enumerate() {
                let next = apply(cur_state, mv);

                let id = self.nodes.len() as u32;
                match self.index.entry(next) {
                    Entry::Occupied(_) => continue,
                    Entry::Vacant(v) => {
                        v.insert(id);
                    }
                }

                self.nodes.push(Node {
                    state: next,
                    parent: cur,
                    mv: mi as u32,
                });
                if other.index.contains_key(&next) {
                    return Some(next);
                }
                next_level.push(id);
            }
        }

        self.level = next_level;
        None
    }
}

fn build_path(
    front: &Search,
    back: &Search,
    meet: State,
    moves: &[Move],
    target: &str,
    cells: usize,
) -> Vec<(String, String)> {
    let mut path = Vec::new();

    let mut cur = front.index[&meet] as usize;
    while front.nodes[cur].parent != NO_PARENT {
        let node = &front.nodes[cur];
        path.push((
            unpack(front.nodes[node.parent as usize].state, cells),
            moves[node.mv as usize].name.clone(),
        ));
        cur = node.parent as usize;
    }
    path.reverse();

    let mut cur = back.index[&meet] as usize;
    while back.nodes[cur].parent != NO_PARENT {
        let node = &back.nodes[cur];
        path.push((
            unpack(node.state, cells),
            moves[node.mv as usize].inverse_name.clone(),
        ));
        cur = node.parent as usize;
    }

    path.push((target.to_string(), String::new()));
    path
}

fn torus(puzzle: &str, n: usize) -> Vec<(String, String)> {
    if n * n > ALPHABET.len() || n > MAX_N {
        return Vec::new();
    }
    let target = &ALPHABET[..n * n];

    if puzzle == target {
        return vec![(target.to_string(), String::new())];
    }

    let mut sorted: Vec<u8> = puzzle.bytes().collect();
    sorted.sort_unstable();
    if n < 2 || sorted != target.as_bytes() {
        return Vec::new();
    }

    let moves = build_moves(n);
    let mut front = Search::new(pack(puzzle.as_bytes()));
    let mut back = Search::new(pack(target.as_bytes()));

    while !front.level.is_empty() && !back.level.is_empty() {
        let meet = if front.level.len() <= back.level.len() {
            front.expand(&back, &moves)
        } else {
            back.expand(&front, &moves)
        };

        if let Some(meet) = meet {
            return build_path(&front, &back, meet, &moves, target, n * n);
        }
    }

    Vec::new()
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let puzzle = args
        .get(1)
        .map(String::as_str)
        .unwrap_or("FGCDEKLHIJOPQMNTUVRSBWXYA");
    let n: usize = args
        .get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| (puzzle.len() as f64).sqrt() as usize);

    let start = Instant::now();
    let result = torus(puzzle, n);
    let elapsed = start.elapsed();

    if result.is_empty() {
        println!("no solution");
    } else {
        println!("moves: {}", result.len() - 1);
    }
    println!("take : {:.6}s", elapsed.as_secs_f64());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replay(path: &[(String, String)], puzzle: &str, n: usize) -> String {
        let moves = build_moves(n);
        let mut s = puzzle.to_string();
        for (state, mv) in path {
            assert_eq!(&s, state);
            if mv.is_empty() {
                break;
            }
            let m = moves.iter().find(|m| &m.name == mv).unwrap();
            s = unpack(apply(pack(s.as_bytes()), m), n * n);
        }
        s
    }

    #[test]
    fn pack_roundtrip() {
        let s = "FGCDEKLHIJOPQMNTUVRSBWXYA";
        assert_eq!(unpack(pack(s.as_bytes()), 25), s);
    }

    #[test]
    fn already_solved() {
        assert_eq!(torus("ABCD", 2), vec![("ABCD".to_string(), String::new())]);
        assert_eq!(torus("A", 1), vec![("A".to_string(), String::new())]);
    }

    #[test]
    fn invalid_puzzle() {
        assert!(torus("AACD", 2).is_empty());
    }

    #[test]
    fn example_4x4_is_valid_and_shortest_known() {
        let puzzle = "ENODLBCHMFGKAIJP";
        let path = torus(puzzle, 4);
        assert_eq!(path.len() - 1, 4);
        assert_eq!(replay(&path, puzzle, 4), &ALPHABET[..16]);
    }

    #[test]
    fn random_scrambles_replay_to_target() {
        let mut seed: u64 = 12345;
        let mut next = move |m: u64| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) % m
        };
        for &n in &[2usize, 3, 4, 5] {
            let moves = build_moves(n);
            for _ in 0..30 {
                let mut s = pack(ALPHABET[..n * n].as_bytes());
                for _ in 0..(next(6) + 1) {
                    s = apply(s, &moves[next(moves.len() as u64) as usize]);
                }
                let puzzle = unpack(s, n * n);
                let path = torus(&puzzle, n);
                assert!(!path.is_empty());
                assert_eq!(replay(&path, &puzzle, n), &ALPHABET[..n * n]);
            }
        }
    }
}
