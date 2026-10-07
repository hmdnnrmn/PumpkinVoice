use crate::state::StateManager;
use pumpkin_plugin_api::{
    command::{Command, CommandNode},
    command_wit::{ArgumentType, StringType},
};
use std::sync::Arc;

pub mod invite;
pub mod join;
pub mod leave;

pub const NAMES: &[&str] = &["voicechat", "vc"];
pub const DESCRIPTION: &str = "Manage simple voice chat settings.";

pub fn init_command_tree(state_manager: Arc<StateManager>) -> Command {
    let join_executor = join::JoinCommandExecutor {
        state_manager: state_manager.clone(),
    };
    let leave_executor = leave::LeaveCommandExecutor {
        state_manager: state_manager.clone(),
    };
    let invite_executor = invite::InviteCommandExecutor {
        state_manager: state_manager.clone(),
    };

    let names_vec: Vec<String> = NAMES.iter().map(|s| s.to_string()).collect();

    Command::new(&names_vec, DESCRIPTION)
        .then(
            CommandNode::literal("join").then(
                CommandNode::argument("group_name", &ArgumentType::String(StringType::SingleWord))
                    .execute(join_executor.clone())
                    .then(
                        CommandNode::argument(
                            "password",
                            &ArgumentType::String(StringType::SingleWord),
                        )
                        .execute(join_executor),
                    ),
            ),
        )
        .then(CommandNode::literal("leave").execute(leave_executor))
        .then(
            CommandNode::literal("invite").then(
                CommandNode::argument("target", &ArgumentType::Players).execute(invite_executor),
            ),
        )
}
