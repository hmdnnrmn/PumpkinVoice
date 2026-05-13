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

    let command = Command::new(&names_vec, DESCRIPTION);

    let join_node = CommandNode::literal("join");
    let group_name_node =
        CommandNode::argument("group_name", &ArgumentType::String(StringType::SingleWord))
            .execute(join_executor.clone());

    let password_node =
        CommandNode::argument("password", &ArgumentType::String(StringType::SingleWord))
            .execute(join_executor);

    group_name_node.then(password_node);
    join_node.then(group_name_node);
    command.then(join_node);

    let leave_node = CommandNode::literal("leave").execute(leave_executor);
    command.then(leave_node);

    let invite_node = CommandNode::literal("invite");
    let target_node =
        CommandNode::argument("target", &ArgumentType::Players).execute(invite_executor);

    invite_node.then(target_node);
    command.then(invite_node);

    command
}
