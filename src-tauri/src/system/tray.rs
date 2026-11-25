use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrayMenuEvent {
    ShowWindow,
    PauseAll,
    ResumeAll,
    Quit,
}
