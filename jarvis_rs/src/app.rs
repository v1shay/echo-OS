use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{self, Align, Layout, RichText, TextEdit};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{error::TryRecvError, UnboundedReceiver, UnboundedSender};

use crate::agent::{AgentCommand, AgentEvent, ApprovalSource};
use crate::config::AppConfig;

pub struct JarvisApp {
    _runtime: Arc<Runtime>,
    config: AppConfig,
    commands: UnboundedSender<AgentCommand>,
    events: UnboundedReceiver<AgentEvent>,
    input: String,
    transcript: String,
    assistant_message: String,
    status: String,
    current_task: String,
    logs: Vec<String>,
    approval_required: Option<String>,
    speech_muted: bool,
    listening: bool,
}

impl JarvisApp {
    pub fn new(
        runtime: Arc<Runtime>,
        config: AppConfig,
        commands: UnboundedSender<AgentCommand>,
        events: UnboundedReceiver<AgentEvent>,
    ) -> Self {
        Self {
            _runtime: runtime,
            config,
            commands,
            events,
            input: String::new(),
            transcript: String::new(),
            assistant_message: "Jarvis is ready.".to_string(),
            status: "Idle".to_string(),
            current_task: "No active task".to_string(),
            logs: vec!["Workspace booted".to_string()],
            approval_required: None,
            speech_muted: false,
            listening: false,
        }
    }

    fn poll_events(&mut self) {
        loop {
            match self.events.try_recv() {
                Ok(event) => self.apply_event(event),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.logs.push("Agent loop disconnected".to_string());
                    break;
                }
            }
        }
    }

    fn apply_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::Status(message) => {
                self.status = message.clone();
                self.logs.push(message);
            }
            AgentEvent::Transcript(text) => {
                self.transcript = text.clone();
                self.logs.push(format!("Transcript: {}", text));
            }
            AgentEvent::AssistantMessage(message) => {
                self.assistant_message = message.clone();
                self.logs.push(format!("Assistant: {}", message));
            }
            AgentEvent::ToolLog(message) => self.logs.push(message),
            AgentEvent::TaskUpdated(summary) => {
                self.current_task = summary.clone();
                self.logs.push(format!("Task: {}", summary));
            }
            AgentEvent::ApprovalRequired(message) => {
                self.approval_required = Some(message.clone());
                self.status = "Awaiting approval".to_string();
                self.logs.push(message);
            }
            AgentEvent::ApprovalResolved(message) => {
                self.approval_required = None;
                self.logs.push(message);
            }
            AgentEvent::Completed(message) => {
                self.status = message.clone();
                self.logs.push(message);
            }
            AgentEvent::Error(message) => {
                self.status = format!("Error: {}", message);
                self.logs.push(self.status.clone());
            }
            AgentEvent::Listening(value) => {
                self.listening = value;
                self.status = if value {
                    "Listening...".to_string()
                } else {
                    "Idle".to_string()
                };
            }
        }
    }

    fn submit_text(&mut self) {
        let input = self.input.trim().to_string();
        if input.is_empty() {
            return;
        }
        self.logs.push(format!("You: {}", input));
        let _ = self.commands.send(AgentCommand::SubmitText(input));
        self.input.clear();
    }
}

impl eframe::App for JarvisApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_events();
        ctx.request_repaint_after(Duration::from_millis(100));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.with_layout(Layout::top_down(Align::LEFT), |ui| {
                ui.heading("Jarvis MVP");
                ui.label(self.config.config_summary());
                ui.separator();

                ui.label(RichText::new("Status").strong());
                ui.label(&self.status);
                ui.label(RichText::new("Current Task").strong());
                ui.label(&self.current_task);
                ui.separator();

                ui.columns(2, |columns| {
                    columns[0].label(RichText::new("Transcript").strong());
                    columns[0].add(
                        TextEdit::multiline(&mut self.transcript)
                            .desired_rows(4)
                            .interactive(false),
                    );

                    columns[1].label(RichText::new("Assistant").strong());
                    columns[1].add(
                        TextEdit::multiline(&mut self.assistant_message)
                            .desired_rows(4)
                            .interactive(false),
                    );
                });

                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Message");
                    let response = ui.add(
                        TextEdit::singleline(&mut self.input)
                            .desired_width(f32::INFINITY)
                            .hint_text("Ask Jarvis to do something on your computer"),
                    );
                    if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter))
                    {
                        self.submit_text();
                    }
                    if ui.button("Send").clicked() {
                        self.submit_text();
                    }
                });

                ui.horizontal(|ui| {
                    if ui.button("Push To Talk").clicked() {
                        let _ = self.commands.send(AgentCommand::StartListening);
                    }
                    if ui
                        .button(if self.speech_muted { "Unmute Speech" } else { "Mute Speech" })
                        .clicked()
                    {
                        self.speech_muted = !self.speech_muted;
                        let _ = self
                            .commands
                            .send(AgentCommand::SetSpeechMuted(self.speech_muted));
                    }
                    if ui.button("Emergency Cancel").clicked() {
                        let _ = self.commands.send(AgentCommand::CancelActive);
                    }
                });

                if let Some(message) = &self.approval_required {
                    ui.separator();
                    ui.label(RichText::new("Approval Needed").strong());
                    ui.label(message);
                    ui.horizontal(|ui| {
                        if ui.button("Approve").clicked() {
                            let _ = self
                                .commands
                                .send(AgentCommand::ApprovePending(ApprovalSource::Desktop));
                        }
                        if ui.button("Reject").clicked() {
                            let _ = self
                                .commands
                                .send(AgentCommand::RejectPending(ApprovalSource::Desktop));
                        }
                    });
                }

                ui.separator();
                ui.label(RichText::new("Activity Log").strong());
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .max_height(260.0)
                    .show(ui, |ui| {
                        for line in &self.logs {
                            ui.label(line);
                        }
                    });
            });
        });
    }
}
