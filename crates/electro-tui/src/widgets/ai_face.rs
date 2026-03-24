//! AI Thinking Face Widget — Shows the AI's current mental state
//!
//! Displays an animated face that changes based on what the AI is doing:
//! - Idle: Calm, breathing animation
//! - Thinking: Active processing animation
//! - Tool Call: Focused/intense expression
//! - Error: Concerned expression

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::theme::Theme;

/// The AI's current emotional/mental state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiMood {
    Idle,
    Thinking,
    Processing,
    ToolCall,
    Error,
    Success,
}

impl AiMood {
    pub fn from_phase(phase: &str) -> Self {
        match phase.to_lowercase().as_str() {
            p if p.contains("idle") => AiMood::Idle,
            p if p.contains("thinking") || p.contains("reasoning") => AiMood::Thinking,
            p if p.contains("processing") || p.contains("calling") => AiMood::Processing,
            p if p.contains("tool") => AiMood::ToolCall,
            p if p.contains("error") || p.contains("failed") => AiMood::Error,
            p if p.contains("complete") || p.contains("success") => AiMood::Success,
            _ => AiMood::Thinking,
        }
    }
}

/// Animated face frames for different moods
pub struct AiFace {
    pub mood: AiMood,
    pub animation_frame: usize,
    pub theme: Theme,
}

impl AiFace {
    pub fn new(theme: Theme) -> Self {
        Self {
            mood: AiMood::Idle,
            animation_frame: 0,
            theme,
        }
    }

    pub fn set_mood(&mut self, mood: AiMood) {
        self.mood = mood;
        self.animation_frame = 0;
    }

    pub fn tick(&mut self) {
        self.animation_frame = self.animation_frame.wrapping_add(1);
    }

    /// Get the ASCII art face based on current mood and animation frame
    fn get_face(&self) -> &'static str {
        match self.mood {
            AiMood::Idle => {
                // Breathing animation
                if self.animation_frame % 4 < 2 {
                    r#"
    ╭─────╮
    │ ◠◠ │
    │  ◡  │
    ╰─────╯
                    "#
                } else {
                    r#"
    ╭─────╮
    │ ◠◠ │
    │  ◡  │
    ╰─────╯
                    "#
                }
            }
            AiMood::Thinking => {
                // Thinking animation (eyes moving)
                match self.animation_frame % 6 {
                    0 | 1 => r#"
    ╭─────╮
    │◠ ◠ │
    │  ◡  │
    ╰─────╯
                    "#,
                    2 | 3 => r#"
    ╭─────╮
    │ ◠◠ │
    │  ◡  │
    ╰─────╯
                    "#,
                    _ => r#"
    ╭─────╮
    │ ◠ ◠│
    │  ◡  │
    ╰─────╯
                    "#,
                }
            }
            AiMood::Processing => {
                // Intense focus
                r#"
    ╭─────╮
    │◉◉│
    │  ◡  │
    ╰─────╯
                "#
            }
            AiMood::ToolCall => {
                // Using tools - determined look
                match self.animation_frame % 3 {
                    0 => r#"
    ╭─────╮
    │◉◉│
    │  ◠  │
    ╰─────╯
                    "#,
                    _ => r#"
    ╭─────╮
    │◉◉│
    │  ◡  │
    ╰─────╯
                    "#,
                }
            }
            AiMood::Error => {
                // Concerned
                r#"
    ╭─────╮
    │◠ ◠│
    │  ◠  │
    ╰─────╯
                "#
            }
            AiMood::Success => {
                // Happy
                match self.animation_frame % 4 {
                    0 | 1 => r#"
    ╭─────╮
    │ ◠◠ │
    │  ◡  │
    ╰─────╯
                    "#,
                    _ => r#"
    ╭─────╮
    │ ◡◡ │
    │  ◡  │
    ╰─────╯
                    "#,
                }
            }
        }
    }

    fn get_status_text(&self) -> &'static str {
        match self.mood {
            AiMood::Idle => "Idle...",
            AiMood::Thinking => "Thinking...",
            AiMood::Processing => "Processing...",
            AiMood::ToolCall => "Using tools...",
            AiMood::Error => "Error occurred",
            AiMood::Success => "Complete!",
        }
    }

    fn get_color(&self) -> Color {
        match self.mood {
            AiMood::Idle => Color::Cyan,
            AiMood::Thinking => Color::Yellow,
            AiMood::Processing => Color::LightYellow,
            AiMood::ToolCall => Color::LightBlue,
            AiMood::Error => Color::Red,
            AiMood::Success => Color::Green,
        }
    }
}

impl Widget for &AiFace {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let face_text = self.get_face();
        let status = self.get_status_text();
        let color = self.get_color();

        let block = Block::default()
            .title(" AI ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(color));

        let inner = block.inner(area);
        block.render(area, buf);

        // Render face
        let face_lines: Vec<Line> = face_text
            .lines()
            .map(|line| Line::from(Span::styled(line, Style::default().fg(color))))
            .collect();

        let face_para = Paragraph::new(face_lines).alignment(Alignment::Center);
        
        // Calculate face area (top half)
        let face_height = face_text.lines().count() as u16;
        let face_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: face_height.min(inner.height),
        };
        face_para.render(face_area, buf);

        // Render status text (bottom)
        if inner.height > face_height {
            let status_area = Rect {
                x: inner.x,
                y: inner.y + face_height,
                width: inner.width,
                height: inner.height - face_height,
            };
            
            let status_para = Paragraph::new(status)
                .alignment(Alignment::Center)
                .style(Style::default().fg(color).add_modifier(Modifier::BOLD));
            status_para.render(status_area, buf);
        }
    }
}

/// Compact thinking indicator for sidebar
pub struct ThinkingIndicator {
    pub is_thinking: bool,
    pub frame: usize,
}

impl ThinkingIndicator {
    pub fn new() -> Self {
        Self {
            is_thinking: false,
            frame: 0,
        }
    }

    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    pub fn set_thinking(&mut self, thinking: bool) {
        self.is_thinking = thinking;
        if thinking {
            self.frame = 0;
        }
    }
}

impl Widget for &ThinkingIndicator {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.is_thinking {
            return;
        }

        let spinner = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        let char_idx = self.frame % spinner.len();
        let spinner_char = spinner[char_idx];

        let text = format!("{} Thinking...", spinner_char);
        let para = Paragraph::new(text)
            .style(Style::default().fg(Color::Yellow));
        
        para.render(area, buf);
    }
}
