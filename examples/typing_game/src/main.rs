//! Typing Speed Game
//!
//! A reactive console typing game demonstrating pulsive's generality.
//! - Sentences loaded from RON files
//! - 10 seconds per sentence
//! - Score increases per correct keystroke
//! - Real-time highlighting of typed characters

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use pulsive_core::{
    effect::{Effect, ModifyOp},
    runtime::{EventHandler, Runtime},
    DefId, EntityRef, Expr, Model, Msg,
};
use std::fs;
use std::io::{stdout, Write};
use std::path::Path;
use std::time::{Duration, Instant};

/// Sentences loaded from RON file
#[derive(serde::Deserialize)]
struct SentenceData {
    sentences: Vec<String>,
}

/// Game constants
const ROUND_DURATION_MS: u64 = 10_000; // 10 seconds
const POINTS_PER_CHAR: i64 = 10;
const BONUS_COMPLETE: i64 = 100;
const TICK_INTERVAL_MS: u64 = 100; // 100ms per tick

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load sentences from RON file
    let sentences = load_sentences()?;
    if sentences.is_empty() {
        eprintln!("No sentences found in data file!");
        return Ok(());
    }

    // Initialize terminal
    terminal::enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, Hide)?;

    // Run game
    let result = run_game(&mut stdout, &sentences);

    // Restore terminal
    execute!(stdout, Show, LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;

    result
}

fn load_sentences() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    // Try multiple paths for the data file
    let paths = [
        "examples/typing_game/data/sentences.ron",
        "data/sentences.ron",
        "../data/sentences.ron",
    ];

    for path in &paths {
        if Path::new(path).exists() {
            let content = fs::read_to_string(path)?;
            let data: SentenceData = ron::from_str(&content)?;
            return Ok(data.sentences);
        }
    }

    Err("Could not find sentences.ron file".into())
}

fn run_game(
    stdout: &mut std::io::Stdout,
    sentences: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    // Create pulsive model and runtime
    let mut model = Model::with_seed(42);
    let mut runtime = Runtime::new();

    // Initialize global game state
    model.set_global("total_score", 0i64);
    model.set_global("rounds_played", 0i64);
    model.set_global("rounds_completed", 0i64);

    // Create the round entity
    let round = model.entities_mut().create("round");
    round.set("sentence", "");
    round.set("typed_index", 0i64);
    round.set("round_score", 0i64);
    round.set("time_remaining_ms", ROUND_DURATION_MS as i64);
    round.set("completed", false);
    round.set("failed", false);
    let round_id = round.id;

    // Register event handler: correct key pressed
    runtime.on_event(EventHandler {
        event_id: DefId::new("correct_key"),
        condition: None,
        effects: vec![
            // Increment typed_index
            Effect::ModifyProperty {
                property: "typed_index".to_string(),
                op: ModifyOp::Add,
                value: Expr::lit(1i64),
            },
            // Add score
            Effect::ModifyProperty {
                property: "round_score".to_string(),
                op: ModifyOp::Add,
                value: Expr::lit(POINTS_PER_CHAR),
            },
        ],
        priority: 0,
    });

    // Register event handler: round complete (sentence fully typed)
    runtime.on_event(EventHandler {
        event_id: DefId::new("round_complete"),
        condition: None,
        effects: vec![
            Effect::SetProperty {
                property: "completed".to_string(),
                value: Expr::lit(true),
            },
            // Add bonus
            Effect::ModifyProperty {
                property: "round_score".to_string(),
                op: ModifyOp::Add,
                value: Expr::lit(BONUS_COMPLETE),
            },
        ],
        priority: 0,
    });

    // Register event handler: round timeout
    runtime.on_event(EventHandler {
        event_id: DefId::new("round_timeout"),
        condition: None,
        effects: vec![Effect::SetProperty {
            property: "failed".to_string(),
            value: Expr::lit(true),
        }],
        priority: 0,
    });

    // Register event handler: time tick (decrease time)
    runtime.on_event(EventHandler {
        event_id: DefId::new("time_tick"),
        condition: None,
        effects: vec![Effect::ModifyProperty {
            property: "time_remaining_ms".to_string(),
            op: ModifyOp::Sub,
            value: Expr::param("delta_ms"),
        }],
        priority: 0,
    });

    // Show welcome screen
    render_welcome(stdout)?;
    wait_for_any_key()?;

    // Game loop - play through sentences
    let mut sentence_index = 0;

    while sentence_index < sentences.len() {
        // Start new round
        let sentence = &sentences[sentence_index];
        start_round(&mut model, round_id, sentence);

        // Round loop
        let _round_start = Instant::now();
        let mut last_tick = Instant::now();

        loop {
            // Check for input (non-blocking)
            if event::poll(Duration::from_millis(10))? {
                if let Event::Key(key_event) = event::read()? {
                    match key_event.code {
                        KeyCode::Esc => {
                            // Exit game
                            return Ok(());
                        }
                        KeyCode::Char('c')
                            if key_event.modifiers.contains(KeyModifiers::CONTROL) =>
                        {
                            return Ok(());
                        }
                        KeyCode::Char(c) => {
                            // Process key press
                            handle_key_press(&mut runtime, &mut model, round_id, c, sentence);
                        }
                        KeyCode::Backspace => {
                            // Allow backspace to correct mistakes
                            handle_backspace(&mut model, round_id);
                        }
                        _ => {}
                    }
                }
            }

            // Time tick
            let now = Instant::now();
            if now.duration_since(last_tick) >= Duration::from_millis(TICK_INTERVAL_MS) {
                let delta = now.duration_since(last_tick).as_millis() as i64;
                last_tick = now;

                // Send time tick event
                let msg = Msg::event("time_tick", EntityRef::Entity(round_id), 0)
                    .with_param("delta_ms", delta);
                runtime.send(msg);
                runtime.process_queue(&mut model);

                // Check for timeout
                let round_entity = model.entities().get(round_id).unwrap();
                let time_remaining =
                    round_entity.get_number("time_remaining_ms").unwrap_or(0.0) as i64;
                if time_remaining <= 0 {
                    let msg = Msg::event("round_timeout", EntityRef::Entity(round_id), 0);
                    runtime.send(msg);
                    runtime.process_queue(&mut model);
                }
            }

            // Render current state
            render_game_state(stdout, &model, round_id, sentence)?;

            // Check for round end
            let round_entity = model.entities().get(round_id).unwrap();
            let completed = round_entity
                .get("completed")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let failed = round_entity
                .get("failed")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if completed || failed {
                // Update global score
                let round_score = round_entity.get_number("round_score").unwrap_or(0.0) as i64;
                let total = model
                    .get_global("total_score")
                    .and_then(|v| v.as_int())
                    .unwrap_or(0);
                model.set_global("total_score", total + round_score);

                let rounds = model
                    .get_global("rounds_played")
                    .and_then(|v| v.as_int())
                    .unwrap_or(0);
                model.set_global("rounds_played", rounds + 1);

                if completed {
                    let completed_count = model
                        .get_global("rounds_completed")
                        .and_then(|v| v.as_int())
                        .unwrap_or(0);
                    model.set_global("rounds_completed", completed_count + 1);
                }

                // Show round result
                render_round_result(stdout, &model, round_id, sentence, completed)?;
                wait_for_any_key()?;
                break;
            }
        }

        sentence_index += 1;
    }

    // Show final results
    render_final_results(stdout, &model)?;
    wait_for_any_key()?;

    Ok(())
}

fn start_round(model: &mut Model, round_id: pulsive_core::EntityId, sentence: &str) {
    let round = model.entities_mut().get_mut(round_id).unwrap();
    round.set("sentence", sentence.to_string());
    round.set("typed_index", 0i64);
    round.set("round_score", 0i64);
    round.set("time_remaining_ms", ROUND_DURATION_MS as i64);
    round.set("completed", false);
    round.set("failed", false);
}

fn handle_key_press(
    runtime: &mut Runtime,
    model: &mut Model,
    round_id: pulsive_core::EntityId,
    typed_char: char,
    sentence: &str,
) {
    let round = model.entities().get(round_id).unwrap();
    let typed_index = round.get_number("typed_index").unwrap_or(0.0) as usize;

    // Check if the typed character matches
    if let Some(expected_char) = sentence.chars().nth(typed_index) {
        if typed_char == expected_char {
            // Correct key!
            let msg = Msg::event("correct_key", EntityRef::Entity(round_id), 0);
            runtime.send(msg);
            runtime.process_queue(model);

            // Check if sentence is complete
            let round = model.entities().get(round_id).unwrap();
            let new_index = round.get_number("typed_index").unwrap_or(0.0) as usize;
            if new_index >= sentence.len() {
                let msg = Msg::event("round_complete", EntityRef::Entity(round_id), 0);
                runtime.send(msg);
                runtime.process_queue(model);
            }
        }
        // Wrong key - no action (could add penalty here)
    }
}

fn handle_backspace(model: &mut Model, round_id: pulsive_core::EntityId) {
    let round = model.entities_mut().get_mut(round_id).unwrap();
    let typed_index = round.get_number("typed_index").unwrap_or(0.0) as i64;
    if typed_index > 0 {
        round.set("typed_index", typed_index - 1);
    }
}

fn wait_for_any_key() -> Result<(), Box<dyn std::error::Error>> {
    loop {
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(_) = event::read()? {
                return Ok(());
            }
        }
    }
}

fn render_welcome(stdout: &mut std::io::Stdout) -> Result<(), Box<dyn std::error::Error>> {
    execute!(stdout, Clear(ClearType::All), MoveTo(0, 0))?;

    let title = r#"
    ╔════════════════════════════════════════════════════════════╗
    ║                                                            ║
    ║              🎮  PULSIVE TYPING GAME  🎮                   ║
    ║                                                            ║
    ║    Demonstrating Reactive Architecture with Pulsive        ║
    ║                                                            ║
    ╠════════════════════════════════════════════════════════════╣
    ║                                                            ║
    ║    HOW TO PLAY:                                            ║
    ║    • Type the sentence shown on screen                     ║
    ║    • You have 10 seconds per sentence                      ║
    ║    • +10 points per correct character                      ║
    ║    • +100 bonus for completing in time                     ║
    ║    • Backspace to correct mistakes                         ║
    ║    • ESC to quit                                           ║
    ║                                                            ║
    ╚════════════════════════════════════════════════════════════╝

                    Press any key to start...
"#;

    execute!(
        stdout,
        SetForegroundColor(Color::Cyan),
        Print(title),
        ResetColor
    )?;
    stdout.flush()?;
    Ok(())
}

fn render_game_state(
    stdout: &mut std::io::Stdout,
    model: &Model,
    round_id: pulsive_core::EntityId,
    sentence: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let round = model.entities().get(round_id).unwrap();
    let typed_index = round.get_number("typed_index").unwrap_or(0.0) as usize;
    let round_score = round.get_number("round_score").unwrap_or(0.0) as i64;
    let time_remaining = round.get_number("time_remaining_ms").unwrap_or(0.0) as i64;
    let total_score = model
        .get_global("total_score")
        .and_then(|v| v.as_int())
        .unwrap_or(0);

    execute!(stdout, Clear(ClearType::All), MoveTo(0, 0))?;

    // Header
    execute!(
        stdout,
        SetForegroundColor(Color::Yellow),
        Print("═══════════════════════════════════════════════════════════════════════\n"),
        Print("                         PULSIVE TYPING GAME                            \n"),
        Print("═══════════════════════════════════════════════════════════════════════\n\n"),
        ResetColor
    )?;

    // Time bar
    let time_secs = time_remaining as f64 / 1000.0;
    let progress = (time_remaining as f64 / ROUND_DURATION_MS as f64).clamp(0.0, 1.0);
    let bar_width = 40;
    let filled = (progress * bar_width as f64) as usize;
    let empty = bar_width - filled;

    let time_color = if time_secs > 5.0 {
        Color::Green
    } else if time_secs > 2.0 {
        Color::Yellow
    } else {
        Color::Red
    };

    execute!(
        stdout,
        Print("  Time: "),
        SetForegroundColor(time_color),
        Print(format!("{:5.1}s ", time_secs)),
        SetBackgroundColor(time_color),
        Print(" ".repeat(filled)),
        SetBackgroundColor(Color::DarkGrey),
        Print(" ".repeat(empty)),
        ResetColor,
        Print("\n\n")
    )?;

    // Score
    execute!(
        stdout,
        Print("  Round Score: "),
        SetForegroundColor(Color::Cyan),
        Print(format!("{:4}", round_score)),
        ResetColor,
        Print("    Total Score: "),
        SetForegroundColor(Color::Magenta),
        Print(format!("{}", total_score)),
        ResetColor,
        Print("\n\n")
    )?;

    // Sentence with highlighting
    execute!(stdout, Print("  "))?;

    for (i, ch) in sentence.chars().enumerate() {
        if i < typed_index {
            // Typed correctly - green background
            execute!(
                stdout,
                SetBackgroundColor(Color::DarkGreen),
                SetForegroundColor(Color::White),
                Print(ch),
                ResetColor
            )?;
        } else if i == typed_index {
            // Current position - cursor highlight
            execute!(
                stdout,
                SetBackgroundColor(Color::Yellow),
                SetForegroundColor(Color::Black),
                Print(ch),
                ResetColor
            )?;
        } else {
            // Not yet typed
            execute!(
                stdout,
                SetForegroundColor(Color::Grey),
                Print(ch),
                ResetColor
            )?;
        }
    }

    execute!(stdout, Print("\n\n"))?;

    // Progress indicator
    let progress_pct = (typed_index as f64 / sentence.len() as f64 * 100.0) as usize;
    execute!(
        stdout,
        Print(format!(
            "  Progress: {}/{} characters ({}%)\n",
            typed_index,
            sentence.len(),
            progress_pct
        ))
    )?;

    // Instructions
    execute!(
        stdout,
        Print("\n"),
        SetForegroundColor(Color::DarkGrey),
        Print("  ESC to quit | Backspace to correct\n"),
        ResetColor
    )?;

    stdout.flush()?;
    Ok(())
}

fn render_round_result(
    stdout: &mut std::io::Stdout,
    model: &Model,
    round_id: pulsive_core::EntityId,
    sentence: &str,
    completed: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let round = model.entities().get(round_id).unwrap();
    let round_score = round.get_number("round_score").unwrap_or(0.0) as i64;
    let typed_index = round.get_number("typed_index").unwrap_or(0.0) as usize;

    execute!(stdout, Clear(ClearType::All), MoveTo(0, 0))?;

    if completed {
        execute!(
            stdout,
            SetForegroundColor(Color::Green),
            Print("\n\n"),
            Print("    ╔═══════════════════════════════════════════════════╗\n"),
            Print("    ║                                                   ║\n"),
            Print("    ║          ✓  ROUND COMPLETE!  ✓                    ║\n"),
            Print("    ║                                                   ║\n"),
            Print("    ╚═══════════════════════════════════════════════════╝\n"),
            ResetColor,
            Print("\n")
        )?;
    } else {
        execute!(
            stdout,
            SetForegroundColor(Color::Red),
            Print("\n\n"),
            Print("    ╔═══════════════════════════════════════════════════╗\n"),
            Print("    ║                                                   ║\n"),
            Print("    ║            ✗  TIME'S UP!  ✗                       ║\n"),
            Print("    ║                                                   ║\n"),
            Print("    ╚═══════════════════════════════════════════════════╝\n"),
            ResetColor,
            Print("\n")
        )?;
    }

    execute!(
        stdout,
        Print(format!("    Sentence: \"{}\"\n\n", sentence)),
        Print(format!(
            "    Characters typed: {}/{}\n",
            typed_index,
            sentence.len()
        )),
        Print(format!("    Round score: {}\n", round_score)),
        Print("\n\n    Press any key to continue...\n")
    )?;

    stdout.flush()?;
    Ok(())
}

fn render_final_results(
    stdout: &mut std::io::Stdout,
    model: &Model,
) -> Result<(), Box<dyn std::error::Error>> {
    let total_score = model
        .get_global("total_score")
        .and_then(|v| v.as_int())
        .unwrap_or(0);
    let rounds_played = model
        .get_global("rounds_played")
        .and_then(|v| v.as_int())
        .unwrap_or(0);
    let rounds_completed = model
        .get_global("rounds_completed")
        .and_then(|v| v.as_int())
        .unwrap_or(0);

    execute!(stdout, Clear(ClearType::All), MoveTo(0, 0))?;

    execute!(
        stdout,
        SetForegroundColor(Color::Cyan),
        Print("\n\n"),
        Print("    ╔═══════════════════════════════════════════════════════════════╗\n"),
        Print("    ║                                                               ║\n"),
        Print("    ║                    🏆  GAME OVER  🏆                          ║\n"),
        Print("    ║                                                               ║\n"),
        Print("    ╚═══════════════════════════════════════════════════════════════╝\n"),
        ResetColor,
        Print("\n\n")
    )?;

    execute!(
        stdout,
        Print(format!("    Final Score:      {}\n", total_score)),
        Print(format!("    Rounds Played:    {}\n", rounds_played)),
        Print(format!("    Rounds Completed: {}\n", rounds_completed)),
        Print("\n")
    )?;

    // Calculate accuracy
    if rounds_played > 0 {
        let accuracy = (rounds_completed as f64 / rounds_played as f64 * 100.0) as i64;
        execute!(
            stdout,
            Print(format!("    Completion Rate:  {}%\n", accuracy))
        )?;
    }

    // Rating
    let rating = if total_score > 2000 {
        "⭐⭐⭐⭐⭐ LEGENDARY!"
    } else if total_score > 1500 {
        "⭐⭐⭐⭐ EXCELLENT!"
    } else if total_score > 1000 {
        "⭐⭐⭐ GREAT!"
    } else if total_score > 500 {
        "⭐⭐ GOOD!"
    } else {
        "⭐ KEEP PRACTICING!"
    };

    execute!(
        stdout,
        Print("\n"),
        SetForegroundColor(Color::Yellow),
        Print(format!("    Rating: {}\n", rating)),
        ResetColor,
        Print("\n\n    Press any key to exit...\n")
    )?;

    stdout.flush()?;
    Ok(())
}
