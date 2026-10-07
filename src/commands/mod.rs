use crate::state::StateManager;
use pumpkin_plugin_api::{
    command::{Command, CommandNode},
    command_wit::{ArgumentType, StringType},
};
use std::sync::Arc;

pub mod help;
pub mod invite;
pub mod join;
pub mod leave;
pub mod status;

pub const NAMES: &[&str] = &["voicechat", "vc"];

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
    let group_name_suggestions = join::GroupNameSuggestionProvider {
        state_manager: state_manager.clone(),
    };
    let status_executor = status::StatusCommandExecutor { state_manager };

    let names_vec: Vec<String> = NAMES.iter().map(|s| s.to_string()).collect();

    // The description is a registration-time plain string in the Pumpkin
    // command API, so it is resolved once in the configured server language.
    let description =
        crate::i18n::translate_str(crate::i18n::default_locale(), "command.description");

    let group_name_node =
        CommandNode::argument("group_name", &ArgumentType::String(StringType::Quotable))
            .suggest(group_name_suggestions)
            .execute(join_executor.clone())
            .then(
                CommandNode::argument("password", &ArgumentType::String(StringType::Quotable))
                    .execute(join_executor),
            );
    let join_node = CommandNode::literal("join").then(group_name_node);

    let leave_node = CommandNode::literal("leave").execute(leave_executor);
    let invite_node = CommandNode::literal("invite")
        .then(CommandNode::argument("target", &ArgumentType::Players).execute(invite_executor));

    Command::new(&names_vec, &description)
        .execute(help::HelpCommandExecutor)
        .then(CommandNode::literal("help").execute(help::HelpCommandExecutor))
        .then(CommandNode::literal("status").execute(status_executor))
        .then(join_node)
        .then(leave_node)
        .then(invite_node)
}
