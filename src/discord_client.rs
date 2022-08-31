use crate::error::BotError;
use crate::utils::env_var;
use crate::utils::secs_to_millis;
use dashmap::DashMap;
use std::env;
use std::num::NonZeroU64;
use std::ops::Deref;
use std::time::Duration;
use tokio::time::Instant;
use twilight_http::response::{DeserializeBodyError, HeaderIter};
use twilight_http::Client;
use twilight_model::channel::{Channel, ChannelType};
use twilight_model::guild::NSFWLevel::Default;
use twilight_model::id::marker::{ApplicationMarker, ChannelMarker};
use twilight_model::id::Id;

struct Fetched<T> {
    item: T,
    fetched: Instant,
}

impl<T> Fetched<T> {
    fn is_expired(&self, duration: &Duration) -> bool {
        &(Instant::now() - self.fetched) > duration
    }

    fn replace(&mut self, item: T) {
        self.item = item;
        self.fetched = Instant::now();
    }

    fn new(item: T) -> Self {
        Self {
            item,
            fetched: Instant::now(),
        }
    }

    fn get(&self) -> &T {
        &self.item
    }
}

pub struct BotDiscordClient {
    application_id: Id<ApplicationMarker>,
    // TODO: this shouldn't all be top-level really
    pub channel_id: Id<ChannelMarker>,
    pub client: Client,
    channels: DashMap<Id<ChannelMarker>, Fetched<Channel>>,
    channel_info_ttl: Duration,
}

#[derive(Debug)]
pub enum DiscordError {
    HttpError(twilight_http::Error),
    ValidationError(String),
    DeserializeBodyError(DeserializeBodyError),
}

impl From<twilight_http::Error> for DiscordError {
    fn from(e: twilight_http::Error) -> Self {
        Self::HttpError(e)
    }
}

impl From<DeserializeBodyError> for DiscordError {
    fn from(e: DeserializeBodyError) -> Self {
        Self::DeserializeBodyError(e)
    }
}

#[derive(Debug)]
pub struct RateLimitInfo {
    pub bucket: String,
    pub reset_after: f64,
    pub remaining: u64,
}

impl RateLimitInfo {
    fn from_headers(headers: HeaderIter) -> Option<Self> {
        let mut builder = RateLimitInfoBuilder::new();
        for (name, val) in headers {
            match name {
                "x-ratelimit-remaining" => {
                    builder.remaining(val);
                }
                "x-ratelimit-reset-after" => {
                    builder.reset_after(val);
                }
                "x-ratelimit-bucket" => {
                    builder.bucket(val);
                }
                _ => {}
            }
        }
        builder.build()
    }

    pub fn reset_after_millis(&self) -> u64 {
        secs_to_millis(self.reset_after)
    }
}

struct RateLimitInfoBuilder<'b> {
    bucket: Option<&'b str>,
    reset_after: Option<f64>,
    remaining: Option<u64>,
}
impl<'b> RateLimitInfoBuilder<'b> {
    fn new() -> Self {
        Self {
            bucket: None,
            reset_after: None,
            remaining: None,
        }
    }
    fn bucket(&mut self, header_value: &'b [u8]) {
        if let Ok(v) = std::str::from_utf8(header_value) {
            self.bucket = Some(v);
        }
    }

    fn reset_after(&mut self, header_value: &'b [u8]) {
        if let Ok(s) = std::str::from_utf8(header_value) {
            if let Ok(v) = s.parse() {
                self.reset_after = Some(v);
            }
        }
    }
    fn remaining(&mut self, header_value: &'b [u8]) {
        if let Ok(s) = std::str::from_utf8(header_value) {
            if let Ok(v) = s.parse() {
                self.remaining = Some(v);
            }
        }
    }
    fn build(self) -> Option<RateLimitInfo> {
        match (self.reset_after, self.remaining, self.bucket) {
            (Some(reset_after), Some(remaining), Some(bucket)) => Some(RateLimitInfo {
                reset_after,
                remaining,
                bucket: bucket.to_string(),
            }),
            _ => None,
        }
    }
}

impl BotDiscordClient {
    pub fn new_from_env() -> Result<Self, BotError> {
        let token = env::var("BOT_TOKEN")?;
        let application_id =
            Id::<ApplicationMarker>::from(env::var("APPLICATION_ID")?.parse::<NonZeroU64>()?);
        let channel_id = Id::<ChannelMarker>::from(env::var("CHANNEL_ID")?.parse::<NonZeroU64>()?);
        let client = Client::new(token);
        let channel_info_ttl = Duration::from_secs(env_var("CHANNEL_INFO_TTL_SECS").parse::<u64>()?);
        Ok(Self {
            client,
            application_id,
            channel_id,
            channels: DashMap::new(),
            channel_info_ttl,
        })
    }

    async fn fetch_channel(&self, id: Id<ChannelMarker>) -> Result<Channel, DiscordError> {
        let resp = self.client.channel(id).exec().await?;
        Ok(resp.model().await?)
    }

    async fn channel<F, O>(&self, id: Id<ChannelMarker>, fn_: F) -> Result<O, DiscordError>
    where
        F: FnOnce(&Channel) -> O,
    {
        let res = match self.channels.get_mut(&id) {
            Some(mut f) => {
                if f.is_expired(&self.channel_info_ttl) {
                    let c = self.fetch_channel(id.clone()).await?;
                    f.replace(c);
                }
                fn_(f.get())
            }
            None => {
                let c = self.fetch_channel(id.clone()).await?;
                let out = fn_(&c);
                let f = Fetched::new(c);
                self.channels.insert(id.clone(), f);
                out
            }
        };
        Ok(res)
    }

    pub async fn create_thread(
        &self,
        thread_name: &str,
    ) -> Result<(Option<RateLimitInfo>, Channel), DiscordError> {
        let archive_duration = self
            .channel(self.channel_id.clone(), |c| c.default_auto_archive_duration)
            .await
            .unwrap_or(None);
        let mut req = self
            .client
            .create_thread(
                self.channel_id.clone(),
                thread_name,
                ChannelType::GuildPublicThread,
            )
            .map_err(|e| DiscordError::ValidationError(e.to_string()))?;
        if let Some(ad) = archive_duration {
            req = req.auto_archive_duration(ad);
        }
        let resp = req.exec().await?;
        let rli = RateLimitInfo::from_headers(resp.headers());
        let channel = resp.model().await?;
        Ok((rli, channel))
    }

    pub async fn create_message(
        &self,
        channel: Id<ChannelMarker>,
        content: &str,
    ) -> Result<Option<RateLimitInfo>, DiscordError> {
        let resp = self
            .client
            .create_message(channel)
            .content(content)
            .map_err(|e| DiscordError::ValidationError(e.to_string()))?
            .exec()
            .await?;
        Ok(RateLimitInfo::from_headers(resp.headers()))
    }
    // TODO: async fn validate_webhook or something like that
}
