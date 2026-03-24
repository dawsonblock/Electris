//! Options/Configuration Panel Widget
//!
//! Displays all available options and settings in a user-friendly panel

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

use crate::theme::Theme;

/// Configuration option types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigOption {
    Toggle { name: String, value: bool, description: String },
    Select { name: String, value: String, options: Vec<String>, description: String },
    Text { name: String, value: String, description: String },
    Button { name: String, label: String, description: String },
}

/// Options panel showing all configurable settings
pub struct OptionsPanel {
    pub is_visible: bool,
    pub selected_index: usize,
    pub options: Vec<ConfigOption>,
    pub theme: Theme,
    pub scroll_offset: usize,
}

impl OptionsPanel {
    pub fn new(theme: Theme) -> Self {
        let options = vec![
            ConfigOption::Select {
                name: "Provider".to_string(),
                value: "kimi".to_string(),
                options: vec!["kimi".to_string(), "anthropic".to_string(), "openai".to_string(), "gemini".to_string()],
                description: "AI provider to use".to_string(),
            },
            ConfigOption::Select {
                name: "Model".to_string(),
                value: "kimi-latest".to_string(),
                options: vec!["kimi-latest".to_string(), "kimi-32k".to_string(), "claude-3-5-sonnet".to_string(), "gpt-4".to_string()],
                description: "Model to use for responses".to_string(),
            },
            ConfigOption::Select {
                name: "Mode".to_string(),
                value: "play".to_string(),
                options: vec!["play".to_string(), "work".to_string(), "pro".to_string()],
                description: "Personality mode".to_string(),
            },
            ConfigOption::Toggle {
                name: "Auto-confirm".to_string(),
                value: false,
                description: "Auto-confirm tool executions".to_string(),
            },
            ConfigOption::Toggle {
                name: "Streaming".to_string(),
                value: true,
                description: "Stream responses token by token".to_string(),
            },
            ConfigOption::Toggle {
                name: "Show AI Face".to_string(),
                value: true,
                description: "Show animated AI thinking face".to_string(),
            },
            ConfigOption::Toggle {
                name: "Sound Effects".to_string(),
                value: false,
                description: "Play sounds on events".to_string(),
            },
            ConfigOption::Text {
                name: "Timeout".to_string(),
                value: "30".to_string(),
                description: "Tool execution timeout (seconds)".to_string(),
            },
            ConfigOption::Button {
                name: "Save Config".to_string(),
                label: "Save".to_string(),
                description: "Save current configuration".to_string(),
            },
            ConfigOption::Button {
                name: "Reset".to_string(),
                label: "Reset to Defaults".to_string(),
                description: "Reset all settings to defaults".to_string(),
            },
        ];

        Self {
            is_visible: false,
            selected_index: 0,
            options,
            theme,
            scroll_offset: 0,
        }
    }

    pub fn toggle(&mut self) {
        self.is_visible = !self.is_visible;
    }

    pub fn show(&mut self) {
        self.is_visible = true;
    }

    pub fn hide(&mut self) {
        self.is_visible = false;
    }

    pub fn next(&mut self) {
        if self.selected_index < self.options.len().saturating_sub(1) {
            self.selected_index += 1;
        }
    }

    pub fn previous(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn toggle_current(&mut self) {
        if let Some(ConfigOption::Toggle { value, .. }) = self.options.get_mut(self.selected_index) {
            *value = !*value;
        }
    }

    pub fn cycle_current(&mut self) {
        if let Some(ConfigOption::Select { value, options, .. }) = self.options.get_mut(self.selected_index) {
            if let Some(current_idx) = options.iter().position(|o| o == value) {
                let next_idx = (current_idx + 1) % options.len();
                *value = options[next_idx].clone();
            }
        }
    }

    fn render_option(&self, option: &ConfigOption, is_selected: bool, area: Rect, buf: &mut Buffer) {
        // Get the foreground color from the theme's text style
        let text_color = match self.theme.text {
            Style { fg: Some(color), .. } => color,
            _ => Color::White,
        };
        let secondary_color = match self.theme.secondary {
            Style { fg: Some(color), .. } => color,
            _ => Color::Gray,
        };

        let select_style = if is_selected {
            Style::default()
                .bg(Color::DarkGray)
                .fg(text_color)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(text_color)
        };

        let normal_style = Style::default().fg(text_color);
        let desc_style = Style::default().fg(secondary_color).add_modifier(Modifier::DIM);

        match option {
            ConfigOption::Toggle { name, value, description } => {
                let checkbox = if *value { "[✓]" } else { "[ ]" };
                let line = Line::from(vec![
                    Span::styled(format!("{} {} ", checkbox, name), select_style),
                    Span::styled(format!("- {}", description), desc_style),
                ]);
                Paragraph::new(line).render(area, buf);
            }
            ConfigOption::Select { name, value, options: _, description } => {
                let line = Line::from(vec![
                    Span::styled(format!("{}: ", name), select_style),
                    Span::styled(value.clone(), normal_style.add_modifier(Modifier::BOLD)),
                    Span::styled(format!(" - {}", description), desc_style),
                ]);
                Paragraph::new(line).render(area, buf);
            }
            ConfigOption::Text { name, value, description } => {
                let line = Line::from(vec![
                    Span::styled(format!("{}: ", name), select_style),
                    Span::styled(value.clone(), normal_style),
                    Span::styled(format!(" - {}", description), desc_style),
                ]);
                Paragraph::new(line).render(area, buf);
            }
            ConfigOption::Button { name: _, label, description } => {
                let btn_style = if is_selected {
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(text_color)
                };
                let line = Line::from(vec![
                    Span::styled(format!("[ {} ]", label), btn_style),
                    Span::styled(format!(" - {}", description), desc_style),
                ]);
                Paragraph::new(line).render(area, buf);
            }
        }
    }
}

impl Widget for &OptionsPanel {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.is_visible {
            return;
        }

        // Clear the area
        Clear.render(area, buf);

        // Create popup in center
        let popup_area = centered_rect(70, 80, area);
        
        let block = Block::default()
            .title(" ⚙️ Configuration ")
            .borders(Borders::ALL)
            .border_style(self.theme.border)
            .title_alignment(Alignment::Center);

        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        // Split into sections
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3),  // Header
                Constraint::Min(10),     // Options list
                Constraint::Length(3),   // Footer
            ])
            .split(inner);

        // Get secondary color for dim text
        let secondary_color = match self.theme.secondary {
            Style { fg: Some(color), .. } => color,
            _ => Color::Gray,
        };

        // Header
        let header = Paragraph::new("Configure your AI assistant")
            .alignment(Alignment::Center)
            .style(Style::default().fg(secondary_color).add_modifier(Modifier::DIM));
        header.render(chunks[0], buf);

        // Options list
        let visible_options = chunks[1].height as usize;
        
        // Adjust scroll offset to keep selection visible
        let scroll_offset = if self.selected_index >= self.scroll_offset + visible_options {
            self.selected_index.saturating_sub(visible_options - 1)
        } else if self.selected_index < self.scroll_offset {
            self.selected_index
        } else {
            self.scroll_offset
        };

        let options_to_show: Vec<_> = self.options
            .iter()
            .skip(scroll_offset)
            .take(visible_options)
            .enumerate()
            .collect();

        for (idx, (display_idx, option)) in options_to_show.iter().enumerate() {
            let actual_idx = scroll_offset + *display_idx;
            let is_selected = actual_idx == self.selected_index;
            
            let option_area = Rect {
                x: chunks[1].x,
                y: chunks[1].y + idx as u16,
                width: chunks[1].width,
                height: 1,
            };
            
            self.render_option(option, is_selected, option_area, buf);
        }

        // Footer with help
        let help_text = "↑/↓: Navigate | Enter: Toggle/Select | q: Close";
        let footer = Paragraph::new(help_text)
            .alignment(Alignment::Center)
            .style(Style::default().fg(secondary_color).add_modifier(Modifier::DIM));
        footer.render(chunks[2], buf);
    }
}

/// Helper to create a centered rect
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
