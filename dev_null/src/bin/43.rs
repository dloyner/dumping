use std::collections::HashMap;
use std::env;
use std::time::Instant;

const ALPHABET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIRECTIONS: [char; 4] = ['U', 'D', 'R', 'L'];

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
    perm: Vec<usize>,
}

fn build_moves(n: usize) -> Vec<Move> {
    let mut moves = Vec::with_capacity(n * 4);
    for idx in 0..n {
        let col: Vec<usize> = (0..n).map(|r| r * n + idx).collect();
        let row: Vec<usize> = (0..n).map(|c| idx * n + c).collect();
        for d in DIRECTIONS {
            let line = if d == 'U' || d == 'D' { &col } else { &row };
            let shift = if d == 'U' || d == 'L' { 1 } else { n - 1 };

            let mut perm: Vec<usize> = (0..n * n).collect();
            for (k, &pos) in line.iter().enumerate() {
                perm[pos] = line[(k + shift) % n];
            }
            moves.push(Move {
                name: format!("{idx}{d}"),
                inverse_name: format!("{idx}{}", inverse(d)),
                perm,
            });
        }
    }
    moves
}

fn apply(state: &[u8], perm: &[usize]) -> Vec<u8> {
    perm.iter().map(|&src| state[src]).collect()
}

struct Node {
    state: Vec<u8>,
    link: Option<(usize, usize)>,
}

struct Search {
    nodes: Vec<Node>,
    index: HashMap<Vec<u8>, usize>,
    level: Vec<usize>,
}

impl Search {
    fn new(root: Vec<u8>) -> Self {
        let mut index = HashMap::new();
        index.insert(root.clone(), 0);
        Search {
            nodes: vec![Node {
                state: root,
                link: None,
            }],
            index,
            level: vec![0],
        }
    }

    fn expand(&mut self, other: &Search, moves: &[Move]) -> Option<Vec<u8>> {
        let mut next_level = Vec::new();

        for &cur in &self.level {
            for (mi, mv) in moves.iter().enumerate() {
                let next = apply(&self.nodes[cur].state, &mv.perm);
                if self.index.contains_key(&next) {
                    continue;
                }

                let id = self.nodes.len();
                self.index.insert(next.clone(), id);
                if other.index.contains_key(&next) {
                    self.nodes.push(Node {
                        state: next.clone(),
                        link: Some((cur, mi)),
                    });
                    return Some(next);
                }
                self.nodes.push(Node {
                    state: next,
                    link: Some((cur, mi)),
                });
                next_level.push(id);
            }
        }

        self.level = next_level;
        None
    }
}

fn to_string(state: &[u8]) -> String {
    state.iter().map(|&b| b as char).collect()
}

fn build_path(
    front: &Search,
    back: &Search,
    meet: &[u8],
    moves: &[Move],
    target: &str,
) -> Vec<(String, String)> {
    let mut path = Vec::new();

    let mut cur = front.index[meet];
    while let Some((parent, mi)) = front.nodes[cur].link {
        path.push((
            to_string(&front.nodes[parent].state),
            moves[mi].name.clone(),
        ));
        cur = parent;
    }
    path.reverse();

    let mut cur = back.index[meet];
    while let Some((parent, mi)) = back.nodes[cur].link {
        path.push((
            to_string(&back.nodes[cur].state),
            moves[mi].inverse_name.clone(),
        ));
        cur = parent;
    }

    path.push((target.to_string(), String::new()));
    path
}

fn torus(puzzle: &str, n: usize) -> Vec<(String, String)> {
    if n * n > ALPHABET.len() {
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
    let mut front = Search::new(puzzle.as_bytes().to_vec());
    let mut back = Search::new(target.as_bytes().to_vec());

    while !front.level.is_empty() && !back.level.is_empty() {
        let meet = if front.level.len() <= back.level.len() {
            front.expand(&back, &moves)
        } else {
            back.expand(&front, &moves)
        };

        if let Some(meet) = meet {
            return build_path(&front, &back, &meet, &moves, target);
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
        for (state, mv) in &result {
            println!("{state}  {mv}");
        }
        println!("\nmoves: {}", result.len() - 1);
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
            s = to_string(&apply(s.as_bytes(), &m.perm));
        }
        s
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
        for &n in &[2usize, 3, 4] {
            let moves = build_moves(n);
            for _ in 0..30 {
                let mut s = ALPHABET[..n * n].as_bytes().to_vec();
                for _ in 0..(next(8) + 1) {
                    s = apply(&s, &moves[next(moves.len() as u64) as usize].perm);
                }
                let puzzle = to_string(&s);
                let path = torus(&puzzle, n);
                assert!(!path.is_empty());
                assert_eq!(replay(&path, &puzzle, n), &ALPHABET[..n * n]);
            }
        }
    }
}
