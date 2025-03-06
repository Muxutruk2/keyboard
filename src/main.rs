use rand::rngs::StdRng;
use rand::SeedableRng;
use rand::seq::SliceRandom;
use rusqlite::{params, Connection, Result};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead};
use tokio::task;
use tokio::sync::Mutex;
use std::sync::Arc;

const ALPHABET: &str = "abcdefghijklmnopqrstuvwxyz";
const MAX_TRIES: usize = 100000000;
const CONCURRENT_TASKS: usize = 13;

#[derive(Debug, Clone)]
struct OptimizationResult {
    layout: Vec<char>,
    cost: f64,
    steps: usize,
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
                let (a, b) = (bigram.chars().next().unwrap(), bigram.chars().nth(1).unwrap());
                bigrams.insert((a, b), freq.unwrap());
            }
        }
    }
    Ok(bigrams)
}

fn calculate_cost(layout: &[char], bigram_freq: &HashMap<(char, char), f64>) -> f64 {
    bigram_freq.iter().fold(0.0, |mut cost, ((a, b), freq)| {
        if let (Some(pos_a), Some(pos_b)) = (
            layout.iter().position(|&x| x == *a),
            layout.iter().position(|&x| x == *b),
        ) {
            cost += (*freq) * (pos_a.abs_diff(pos_b) as f64);
        }
        cost
    })
}

fn generate_random_layout(rng: &mut StdRng) -> Vec<char> {
    let mut layout: Vec<char> = ALPHABET.chars().collect();
    layout.shuffle(rng);
    layout
}

async fn layout_exists(conn: Arc<Mutex<Connection>>, layout: &[char]) -> bool {
    let conn = conn.lock().await;
    let existing: Result<String> = conn.query_row(
        "SELECT layout FROM layouts WHERE layout = ?1",
        params![layout.iter().collect::<String>()],
        |row| row.get(0),
    );
    existing.is_ok()
}

fn find_valley(
    mut layout: Vec<char>,
    bigram_freq: &HashMap<(char, char), f64>,
) -> OptimizationResult {
    let mut current_cost = calculate_cost(&layout, bigram_freq);
    let mut steps = 0;
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
        steps += 1;
        if let Some((i, j)) = best_swap {
            layout.swap(i, j);
            current_cost = best_swap_cost;
        } else {
            return OptimizationResult {
                layout,
                cost: current_cost,
                steps,
            };
        }
    }
}

async fn save_to_db(conn: Arc<Mutex<Connection>>, result: OptimizationResult) -> Result<()> {
    if layout_exists(conn.clone(), &result.layout).await {
        return Ok(());
    }
    let conn = conn.lock().await;
    conn.execute(
        "INSERT INTO layouts (layout, cost, steps) VALUES (?1, ?2, ?3)",
        params![result.layout.iter().collect::<String>(), result.cost, result.steps],
    )?;
    Ok(())
}

fn setup_db() -> Result<Connection> {
    let conn = Connection::open("layouts.db")?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS layouts (
id INTEGER PRIMARY KEY,
layout TEXT UNIQUE,
cost REAL,
steps INTEGER
)",
        [],
    )?;
    Ok(conn)
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let bigram_freq = load_bigram_frequencies("bigrams.txt")?;
    let conn = Arc::new(Mutex::new(setup_db().expect("Failed to set up database")));

    let mut tasks = vec![];
    for _ in 0..CONCURRENT_TASKS {
        let bigram_freq = bigram_freq.clone();
        let conn = Arc::clone(&conn);
        tasks.push(task::spawn(async move {
            for _ in 0..(MAX_TRIES / CONCURRENT_TASKS) {
                let mut rng = StdRng::from_rng(&mut rand::rng());
                let initial_layout = generate_random_layout(&mut rng);
                let valley = find_valley(initial_layout, &bigram_freq);
                if !layout_exists(conn.clone(), &valley.layout).await {
                    println!(
                        "Found valley: {:?} with cost: {}. Steps {}",
                        valley.layout.iter().collect::<String>(),
                        valley.cost,
                        valley.steps,
                    );
                    save_to_db(conn.clone(), valley).await.map_err(|e| eprintln!("Failed to save to DB: {e}"));
                }
            }
        }));
    }

    for t in tasks {
        t.await.unwrap();
    }

    Ok(())
}
