use crate::error::BotError;
use crate::utils::secs_to_millis;
use std::env;
use std::num::NonZeroU64;
use std::time::Duration;
use twilight_http::error::ErrorType;
use twilight_http::response::{DeserializeBodyError, HeaderIter};
use twilight_http::{Client, Error, Response};
use twilight_model::channel::embed::Embed;
use twilight_model::channel::Channel;
use twilight_model::id::marker::{ApplicationMarker, ChannelMarker};
use twilight_model::id::Id;

pub struct BotDiscordClient {
    application_id: Id<ApplicationMarker>,
    // TODO: this shouldn't all be top-level really
    pub channel_id: Id<ChannelMarker>,
    pub client: Client,
}

#[derive(Debug, thiserror::Error)]
pub enum DiscordError {
    /// error getting a response from the API
    #[error("API Error: {0}")]
    HttpError(#[from] Error),
    /// error validating something (message too long, etc)
    #[error("Validation error: {0}")]
    ValidationError(String),
    /// body returned in an otherwise-valid response didn't deserialize properly
    #[error("Deserialization error in otherwise valid response: {0}")]
    DeserializeBodyError(#[from] DeserializeBodyError),
    /// caller provided bad input
    #[error("Programmer error invalid input: {0}")]
    InvalidInput(#[from] InvalidInputError),
}

#[derive(Debug, thiserror::Error)]
pub enum InvalidInputError {
    #[error("Expected thread, got something else (channel?)")]
    ThatsNotAThread,
}

impl DiscordError {
    pub fn is_404(&self) -> bool {
        match self {
            DiscordError::HttpError(httpe) => match httpe.kind() {
                ErrorType::Response { status, .. } => status.get() == 404,
                _ => false,
            },
            _ => false,
        }
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

#[derive(Debug)]
pub struct WithRateLimitInfo<T> {
    rli: Option<RateLimitInfo>,
    pub item: T,
}

impl<T> WithRateLimitInfo<T> {
    fn new<R>(item: T, resp: &Response<R>) -> Self {
        Self {
            rli: RateLimitInfo::from_headers(resp.headers()),
            item,
        }
    }

    /// if we have rate limiting info, and we're out of requests, this returns the duration we
    /// should sleep for
    /// if we are missing info or have requests left, returns None (so None might be sort of a lie)
    pub fn sleep_time(&self) -> Option<Duration> {
        if let Some(rli) = &self.rli {
            if rli.remaining == 0 {
                return Some(Duration::from_millis(rli.reset_after_millis()));
            }
        }
        None
    }

    /// sleeps for the amount of time discord told us to, if any
    pub async fn sleep(&self) {
        if let Some(sleep_time) = self.sleep_time() {
            tokio::time::sleep(sleep_time).await;
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
        Ok(Self {
            client,
            application_id,
            channel_id,
        })
    }

    /// Fetches a channel from discord by ID (no caching)
    pub async fn fetch_channel(&self, id: Id<ChannelMarker>) -> Result<Channel, DiscordError> {
        let resp = self.client.channel(id).exec().await?;
        Ok(resp.model().await?)
    }

    pub async fn create_message(
        &self,
        embeds: Vec<Embed>,
    ) -> Result<WithRateLimitInfo<()>, DiscordError> {
        let resp = self
            .client
            .create_message(self.channel_id)
            .embeds(&embeds)
            .map_err(|e| DiscordError::ValidationError(e.to_string()))?
            .exec()
            .await?;
        Ok(WithRateLimitInfo::new((), &resp))
    }
}
