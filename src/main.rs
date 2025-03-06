use rand::{seq::SliceRandom, Rng};
use rusqlite::{params, Connection, Result};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead};

const ALPHABET: &str = "abcdefghijklmnopqrstuvwxyz";
const MAX_TRIES: usize = 100000;

#[derive(Debug, Clone)]
struct OptimizationResult {
    layout: Vec<char>,
    cost: f64,
}

fn load_bigram_frequencies(filename: &str) -> io::Result<HashMap<(char, char), f64>> {
    let file = File::open(filename)?;
    let reader = io::BufReader::new(file);
    let mut bigrams = HashMap::new();

    for line in reader.lines() {
        let line = line?;
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() == 2 {
            let bigram = parts[0];
            let freq: Result<f64, _> = parts[1].parse();
            if bigram.len() == 2 && freq.is_ok() {
                let (a, b) = (
                    bigram.chars().next().unwrap(),
                    bigram.chars().nth(1).unwrap(),
                );
                bigrams.insert((a, b), freq.unwrap());
            }
        }
    }
    Ok(bigrams)
}

fn calculate_cost(layout: &[char], bigram_freq: &HashMap<(char, char), f64>) -> f64 {
    let mut cost = 0.0;
    for ((a, b), freq) in bigram_freq.iter() {
        if let (Some(pos_a), Some(pos_b)) = (
            layout.iter().position(|&x| x == *a),
            layout.iter().position(|&x| x == *b),
        ) {
            cost += (*freq) * (pos_a.abs_diff(pos_b) as f64);
        }
    }
    cost
}

fn generate_random_layout() -> Vec<char> {
    let mut layout: Vec<char> = ALPHABET.chars().collect();
    let mut rng = rand::rng();
    layout.shuffle(&mut rng);
    layout
}

fn find_valley(
    mut layout: Vec<char>,
    bigram_freq: &HashMap<(char, char), f64>,
) -> OptimizationResult {
    let mut current_cost = calculate_cost(&layout, bigram_freq);
    loop {
        let mut best_swap = None;
        let mut best_swap_cost = current_cost;

        for i in 0..26 {
            for j in i + 1..26 {
                layout.swap(i, j);
                let new_cost = calculate_cost(&layout, bigram_freq);
                if new_cost < best_swap_cost {
                    best_swap = Some((i, j));
                    best_swap_cost = new_cost;
                }
                layout.swap(i, j);
            }
        }

        if let Some((i, j)) = best_swap {
            layout.swap(i, j);
            current_cost = best_swap_cost;
        } else {
            return OptimizationResult {
                layout,
                cost: current_cost,
            };
        }
    }
}

fn save_to_db(conn: &Connection, result: &OptimizationResult) -> Result<()> {
    let existing: Result<String> = conn.query_row(
        "SELECT layout FROM layouts WHERE layout = ?1",
        params![result.layout.iter().collect::<String>()],
        |row| row.get(0),
    );

    if existing.is_err() {
        conn.execute(
            "INSERT INTO layouts (layout, cost) VALUES (?1, ?2)",
            params![result.layout.iter().collect::<String>(), result.cost],
        )?;
    }
    Ok(())
}

fn setup_db() -> Result<Connection> {
    let conn = Connection::open("layouts.db")?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS layouts (
            id INTEGER PRIMARY KEY,
            layout TEXT UNIQUE,
            cost REAL
        )",
        [],
    )?;
    Ok(conn)
}

fn main() -> io::Result<()> {
    let bigram_freq = load_bigram_frequencies("bigrams.txt")?;
    let conn = setup_db().expect("Failed to set up database");

    for _ in 0..MAX_TRIES {
        let initial_layout = generate_random_layout();
        let valley = find_valley(initial_layout, &bigram_freq);
        println!(
            "Found valley: {:?} with cost: {:.2}",
            valley.layout.iter().collect::<String>(),
            valley.cost
        );
        save_to_db(&conn, &valley).expect("Failed to save to DB");
    }

    Ok(())
}
