use alttp_queue_bot::discord_client::BotDiscordClient;
use std::time::Duration;
use twilight_http::request::TryIntoRequest;
use twilight_model::channel::ChannelType;

#[tokio::main]
async fn main() {
    dotenv::dotenv().unwrap();
    let c = BotDiscordClient::new_from_env().unwrap();
    c.create_thread("test1").await;
    c.create_thread("test2").await;
    tokio::time::sleep(Duration::from_secs(10)).await;

    c.create_thread("test3").await;
    //
    // let chan = c.client.channel(c.channel_id.clone())
    //     .exec()
    //     .await
    //     .unwrap();
    // let chan_mod = chan.model().await.unwrap();
    // println!("{:?}", chan_mod);
    //
    // let ct = c.client
    //     .create_thread(
    //         c.channel_id.clone(),
    //         "test",
    //         ChannelType::GuildPublicThread
    //     ).unwrap();
    // let thing = ct.try_into_request().unwrap();
    // println!("{:?}", thing);
    // println!("body: {}", String::from_utf8_lossy(thing.body().unwrap()));
    //
    // println!("create thread");
}
