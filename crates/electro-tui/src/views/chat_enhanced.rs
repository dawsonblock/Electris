//! Enhanced chat view with AI face and options panel.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Widget};

use electro_agent::agent_task_status::AgentTaskPhase;

use crate::app::AppState;
use crate::widgets::ai_face::AiMood;
use crate::widgets::input_area::InputArea;
// OptionsPanel is accessed through AppState
use crate::widgets::status_bar::{StatusBar, StatusBarData};

/// Render the enhanced chat view with AI face sidebar.
pub fn render_chat_enhanced(state: &mut AppState, area: Rect, buf: &mut Buffer) {
    // Main horizontal split: sidebar | chat area
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(if state.show_ai_face { 20 } else { 0 }), // AI face sidebar
            Constraint::Percentage(100),                               // Main chat area
        ])
        .split(area);

    // === AI Face Sidebar (left) ===
    if state.show_ai_face {
        let sidebar_area = main_chunks[0];
        
        // Update AI face mood based on agent state
        let mood = if state.is_agent_working {
            match &state.activity_panel.phase {
                AgentTaskPhase::ExecutingTool { .. } => AiMood::ToolCall,
                AgentTaskPhase::CallingProvider { .. } => AiMood::Processing,
                _ => AiMood::Thinking,
            }
        } else {
            AiMood::Idle
        };
        state.ai_face.set_mood(mood);
        state.ai_face.tick();
        
        // Render AI face widget
        (&state.ai_face).render(sidebar_area, buf);
    }

    // === Main Chat Area (right) ===
    let chat_area = if state.show_ai_face { main_chunks[1] } else { area };
    
    // Vertical layout within chat area
    let activity_height = state.activity_panel.height();
    let thinking_height = if state.is_agent_working && activity_height == 0 {
        1
    } else {
        activity_height
    };
    let input_height = (state.input.lines.len() as u16).clamp(1, 10);
    let status_height = 1u16;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(thinking_height),
            Constraint::Length(input_height + 1),
            Constraint::Length(status_height),
        ])
        .split(chat_area);

    // === Messages ===
    let msg_area = chunks[0];
    let view_height = msg_area.height as usize;

    // Render all message lines
    let mut all_lines: Vec<Line<'static>> = state.message_list.render_lines(
        state.theme.prompt,
        state.theme.text,
        state.theme.secondary,
        state.theme.secondary,
    );

    // Append in-progress streaming content if agent is working
    if state.is_agent_working {
        if let Some(ref renderer) = state.streaming_renderer {
            if !renderer.is_empty() {
                for rl in renderer.lines() {
                    all_lines.push(Line::from(rl.spans.clone()));
                }
            }
        }
    }

    let total = all_lines.len();

    // Scroll logic
    let max_offset = total.saturating_sub(view_height);
    let offset = state.message_list.scroll_offset.min(max_offset);
    let end = total.saturating_sub(offset);
    let start = end.saturating_sub(view_height);

    for (i, line) in all_lines[start..end].iter().enumerate() {
        let y = msg_area.top() + i as u16;
        if y < msg_area.bottom() {
            buf.set_line(msg_area.left(), y, line, msg_area.width);
        }
    }

    // Scroll indicator
    if offset > 0 && msg_area.width > 20 {
        let indicator = format!(" ↑ {} more lines ", offset);
        let ind_line = Line::from(Span::styled(
            indicator,
            state.theme.secondary.add_modifier(Modifier::DIM),
        ));
        buf.set_line(
            msg_area.right().saturating_sub(25),
            msg_area.bottom().saturating_sub(1),
            &ind_line,
            25,
        );
    }

    // === Activity/thinking indicator ===
    if activity_height > 0 {
        let panel_lines = state.activity_panel.render_lines(
            state.theme.phase_done,
            state.theme.phase_active,
            state.theme.phase_pending,
            state.theme.tool_running,
            state.theme.info,
            state.theme.error,
        );
        for (i, line) in panel_lines.iter().enumerate() {
            let y = chunks[1].top() + i as u16;
            if y < chunks[1].bottom() {
                buf.set_line(chunks[1].left(), y, line, chunks[1].width);
            }
        }
    } else if state.is_agent_working && thinking_height > 0 {
        let elapsed = state.activity_panel.started_at.elapsed();
        let phase_text = match &state.activity_panel.phase {
            AgentTaskPhase::Preparing => "Preparing",
            AgentTaskPhase::Classifying => "Classifying",
            AgentTaskPhase::CallingProvider { round } => {
                if *round <= 1 {
                    "Thinking"
                } else {
                    "Thinking (multi-round)"
                }
            }
            AgentTaskPhase::ExecutingTool { tool_name, .. } => tool_name.as_str(),
            AgentTaskPhase::Finishing => "Finishing",
            AgentTaskPhase::Done => "Done",
            AgentTaskPhase::Interrupted { .. } => "Interrupted",
        };
        let spinner_char = match (elapsed.as_millis() / 200) % 4 {
            0 => "◜",
            1 => "◝",
            2 => "◞",
            _ => "◟",
        };
        let line = Line::from(vec![
            Span::styled(format!(" {} ", spinner_char), state.theme.phase_active),
            Span::styled(phase_text.to_string(), state.theme.phase_active),
            Span::styled(
                format!("  {:.1}s", elapsed.as_secs_f64()),
                state.theme.secondary.add_modifier(Modifier::DIM),
            ),
        ]);
        buf.set_line(chunks[1].left(), chunks[1].top(), &line, chunks[1].width);
    }

    // === Input ===
    let input_block = Block::default()
        .borders(Borders::TOP)
        .border_style(state.theme.border);
    let input_inner = input_block.inner(chunks[2]);
    input_block.render(chunks[2], buf);

    InputArea::new(&state.input)
        .prompt("tem> ")
        .prompt_style(state.theme.prompt)
        .text_style(state.theme.text)
        .cursor_style(state.theme.input_cursor)
        .render(input_inner, buf);

    // === Status bar ===
    let status_data = StatusBarData {
        model: state.current_model.clone().unwrap_or_default(),
        provider: state.current_provider.clone().unwrap_or_default(),
        total_input_tokens: state.token_counter.total_input_tokens,
        total_output_tokens: state.token_counter.total_output_tokens,
        total_cost_usd: state.token_counter.total_cost_usd,
        is_agent_working: state.is_agent_working,
    };
    StatusBar::new(
        &status_data,
        state.theme.status_bar,
        state.theme.accent,
        state.theme.info,
    )
    .render(chunks[3], buf);

    // === Options Panel Overlay ===
    if state.options_panel.is_visible {
        (&state.options_panel).render(area, buf);
    }
}

/// Toggle AI face visibility
pub fn toggle_ai_face(state: &mut AppState) {
    state.show_ai_face = !state.show_ai_face;
    state.needs_redraw = true;
}

/// Toggle options panel
pub fn toggle_options_panel(state: &mut AppState) {
    state.options_panel.toggle();
    state.needs_redraw = true;
}
