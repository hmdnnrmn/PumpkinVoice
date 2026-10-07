use pumpkin_plugin_api::{
    Server,
    command::{CommandError, CommandSender, ConsumedArgs},
    commands::CommandHandler,
};

pub struct HelpCommandExecutor;

impl CommandHandler for HelpCommandExecutor {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        _args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let locale = sender.as_player().map_or_else(
            || crate::i18n::default_locale().to_string(),
            |player| player.get_locale(),
        );

        for key in [
            "command.help.header",
            "command.help.help",
            "command.help.status",
            "command.help.join",
            "command.help.leave",
            "command.help.invite",
        ] {
            sender.send_message(crate::i18n::tr(&locale, key));
        }

        Ok(1)
    }
}
