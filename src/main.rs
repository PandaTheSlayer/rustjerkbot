use carapax::{
    context::Context,
    core::{types::Update, Api, UpdateMethod, UpdatesStream},
    App, CommandsHandler, FnHandler,
};
use carapax_access::{AccessHandler, AccessRule, InMemoryAccessPolicy};
use dotenv::dotenv;
use env_logger;
use futures::{future, Future};

mod config;
mod entities;
mod handler;
mod sender;
mod store;
mod utils;

use self::{
    config::Config,
    handler::{
        autoresponse::AutoresponseHandler,
        ferris::handle_ferris,
        shippering::ShipperingHandler,
        text::{replace_text_handler, TransformCommand},
        tracker::track_chat_member,
        user::get_user_info,
    },
    sender::MessageSender,
    store::{autoresponse::MessageStore, db::Store, shippering::TemplateStore},
};

fn main() {
    dotenv().ok();
    env_logger::init();

    let config = Config::from_env().expect("Can not read configuration file");
    let api = Api::new(config.get_api_config()).expect("Can not to create Api");

    let msg_store =
        MessageStore::from_file("data/messages.yml").expect("Failed to create message store");

    let tpl_store =
        TemplateStore::from_file("data/shippering.yml").expect("Failed to create template store");

    let access_rule = AccessRule::allow_chat(config.chat_id);
    let access_policy = InMemoryAccessPolicy::default().push_rule(access_rule);

    let update_method = match config.webhook_url.clone() {
        Some((addr, path)) => {
            log::info!("Started receiving updates via webhook: {}{}", addr, path);
            UpdateMethod::webhook(addr, path)
        }
        None => {
            log::info!("Started receiving updates via long polling");
            UpdateMethod::poll(UpdatesStream::new(api.clone()))
        }
    };

    tokio::run(future::lazy(move || {
        Store::open(config.redis_url.clone())
            .map_err(|e| log::error!("Unable to open store: {:?}", e))
            .and_then(move |store| {
                let message_sender = MessageSender::new(api.clone(), store.clone());
                let setup_context = move |context: &mut Context, _update: Update| {
                    context.set(store.clone());
                    context.set(config.clone());
                    context.set(message_sender.clone())
                };
                App::new()
                    .add_handler(AccessHandler::new(access_policy))
                    .add_handler(FnHandler::from(setup_context))
                    .add_handler(FnHandler::from(track_chat_member))
                    .add_handler(FnHandler::from(replace_text_handler))
                    .add_handler(AutoresponseHandler::new(msg_store))
                    .add_handler(
                        CommandsHandler::default()
                            .add_handler("/shippering", ShipperingHandler::new(tpl_store))
                            .add_handler("/arrow", TransformCommand::arrow())
                            .add_handler("/cw", TransformCommand::cw())
                            .add_handler("/fsays", handle_ferris)
                            .add_handler("/huify", TransformCommand::huify())
                            .add_handler("/reverse", TransformCommand::reverse())
                            .add_handler("/square", TransformCommand::square())
                            .add_handler("/star", TransformCommand::star())
                            .add_handler("/user", get_user_info),
                    )
                    .run(api, update_method)
            })
    }));
}
