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

    let password_node =
        CommandNode::argument("password", &ArgumentType::String(StringType::SingleWord))
            .execute(join_executor.clone());

    let group_name_node =
        CommandNode::argument("group_name", &ArgumentType::String(StringType::SingleWord))
            .execute(join_executor)
            .then(password_node);

    let join_node = CommandNode::literal("join").then(group_name_node);

    let leave_node = CommandNode::literal("leave").execute(leave_executor);

    let target_node =
        CommandNode::argument("target", &ArgumentType::Players).execute(invite_executor);
    let invite_node = CommandNode::literal("invite").then(target_node);

    Command::new(&names_vec, DESCRIPTION)
        .then(join_node)
        .then(leave_node)
        .then(invite_node)
}
