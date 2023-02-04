use crate::error::BotError;
use crate::utils::env_var;
use crate::utils::secs_to_millis;
use dashmap::DashMap;
use std::env;
use std::num::NonZeroU64;
use std::time::Duration;
use tokio::time::Instant;
use twilight_http::response::{DeserializeBodyError, HeaderIter};
use twilight_http::{Client, Error, Response};
use twilight_http::error::ErrorType;
use twilight_model::channel::{Channel, ChannelType};
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
    InvalidInput(#[from] InvalidInputError)
}

#[derive(Debug, thiserror::Error)]
pub enum InvalidInputError {
    #[error("Expected thread, got something else (channel?)")]
    ThatsNotAThread,
}


impl DiscordError {
    pub fn is_404(&self) -> bool {
        match self {
            DiscordError::HttpError(httpe) => {
                match httpe.kind() {
                    ErrorType::Response {  status, .. } => {
                        status.get() == 404
                    }
                    _ => false
                }
            }
            _ => false
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
            item
        }
    }

    /// `resp.model()` consumes `resp` so you might have to build the RLI before constructing
    /// this object
    fn with_rli(item: T, rli: Option<RateLimitInfo>) -> Self {
        Self {
            rli,
            item
        }
    }

    fn new_no_rli(item: T) -> Self {
        Self {
            rli: None,
            item
        }
    }

    /// if we have rate limiting info, and we're out of requests, this returns the duration we
    /// should sleep for
    /// if we are missing info or have requests left, returns None (so None might be sort of a lie)
    pub fn sleep_time(&self) -> Option<Duration> {
        if let Some(rli) = &self.rli {
            if rli.remaining == 0 {
                return Some(Duration::from_millis(rli.reset_after_millis()))
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
        let channel_info_ttl = Duration::from_secs(env_var("CHANNEL_INFO_TTL_SECS").parse::<u64>()?);
        Ok(Self {
            client,
            application_id,
            channel_id,
            channels: DashMap::new(),
            channel_info_ttl,
        })
    }

    /// Fetches a channel from discord by ID (no caching)
    pub async fn fetch_channel(&self, id: Id<ChannelMarker>) -> Result<Channel, DiscordError> {
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
    ) -> Result<WithRateLimitInfo<Channel>, DiscordError> {
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
        Ok(WithRateLimitInfo::with_rli(channel, rli))
    }

    // no real reason for this to be char instead of str but it's convenient
    /// returns true if we did any work, false if the thread was already archived
    pub async fn finalize_thread(&self, id: Id<ChannelMarker>, new_prefix: char) -> Result<WithRateLimitInfo<bool>, DiscordError> {
        let existing_thread = self.fetch_channel(id).await?;

        if let Some(tmd) = existing_thread.thread_metadata {
            if tmd.archived {
                return Ok(WithRateLimitInfo::new_no_rli(false));
            }
        } else {
            return Err(DiscordError::InvalidInput(InvalidInputError::ThatsNotAThread));
        }
        println!("Updating {:?}", existing_thread.name);
        // i dont know how thread name could be null? but apparently it can. discord api says so.
        let resp = self.client.update_thread(id)
            .name(&format!("{} {}",new_prefix, existing_thread.name.unwrap_or("-".to_string())))
            .map_err(|e| DiscordError::ValidationError(e.to_string()))?
            .archived(true)
            .exec()
            .await?;
        Ok(WithRateLimitInfo::new(true, &resp))
    }

    pub async fn create_message(
        &self,
        channel: Id<ChannelMarker>,
        content: &str,
    ) -> Result<WithRateLimitInfo<()>, DiscordError> {
        let resp = self
            .client
            .create_message(channel)
            .content(content)
            .map_err(|e| DiscordError::ValidationError(e.to_string()))?
            .exec()
            .await?;
        Ok(WithRateLimitInfo::new((), &resp))
    }

}
