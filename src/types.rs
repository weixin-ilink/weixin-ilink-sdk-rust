use serde::{Deserialize, Serialize};

// ── Enums ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "u8", try_from = "u8")]
pub enum MessageItemType {
    Text = 1,
    Image = 2,
    Voice = 3,
    File = 4,
    Video = 5,
}

impl From<MessageItemType> for u8 {
    fn from(v: MessageItemType) -> u8 {
        v as u8
    }
}

impl TryFrom<u8> for MessageItemType {
    type Error = u8;
    fn try_from(v: u8) -> Result<Self, u8> {
        match v {
            1 => Ok(Self::Text),
            2 => Ok(Self::Image),
            3 => Ok(Self::Voice),
            4 => Ok(Self::File),
            5 => Ok(Self::Video),
            _ => Err(v),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "u8", try_from = "u8")]
pub enum UploadMediaType {
    Image = 1,
    Video = 2,
    File = 3,
    Voice = 4,
}

impl From<UploadMediaType> for u8 {
    fn from(v: UploadMediaType) -> u8 {
        v as u8
    }
}

impl TryFrom<u8> for UploadMediaType {
    type Error = u8;
    fn try_from(v: u8) -> Result<Self, u8> {
        match v {
            1 => Ok(Self::Image),
            2 => Ok(Self::Video),
            3 => Ok(Self::File),
            4 => Ok(Self::Voice),
            _ => Err(v),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "u8", try_from = "u8")]
pub enum MessageType {
    None = 0,
    User = 1,
    Bot = 2,
}

impl From<MessageType> for u8 {
    fn from(v: MessageType) -> u8 {
        v as u8
    }
}

impl TryFrom<u8> for MessageType {
    type Error = u8;
    fn try_from(v: u8) -> Result<Self, u8> {
        match v {
            0 => Ok(Self::None),
            1 => Ok(Self::User),
            2 => Ok(Self::Bot),
            _ => Err(v),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "u8", try_from = "u8")]
pub enum MessageState {
    New = 0,
    Generating = 1,
    Finish = 2,
}

impl From<MessageState> for u8 {
    fn from(v: MessageState) -> u8 {
        v as u8
    }
}

impl TryFrom<u8> for MessageState {
    type Error = u8;
    fn try_from(v: u8) -> Result<Self, u8> {
        match v {
            0 => Ok(Self::New),
            1 => Ok(Self::Generating),
            2 => Ok(Self::Finish),
            _ => Err(v),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypingStatus {
    Typing = 1,
    Cancel = 2,
}

impl From<TypingStatus> for u8 {
    fn from(v: TypingStatus) -> u8 {
        v as u8
    }
}

// ── Common ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BaseInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_version: Option<String>,
}

// ── CDN Media ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CdnMedia {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypt_query_param: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aes_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypt_type: Option<u8>,
}

// ── Message Items ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TextItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImageItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<CdnMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_media: Option<CdnMedia>,
    /// Raw AES-128 key as hex string; preferred for inbound decryption.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aeskey: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mid_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hd_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VoiceItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<CdnMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encode_type: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bits_per_sample: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playtime: Option<u64>,
    /// Voice-to-text transcription.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<CdnMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub md5: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub len: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VideoItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<CdnMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play_length: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_md5: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_media: Option<CdnMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_width: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RefMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_item: Option<MessageItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MessageItem {
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_type: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_completed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_msg: Option<Box<RefMessage>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_item: Option<TextItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_item: Option<ImageItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_item: Option<VoiceItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_item: Option<FileItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_item: Option<VideoItem>,
}

impl MessageItem {
    pub fn typed(&self) -> Option<MessageItemType> {
        self.item_type.and_then(|v| MessageItemType::try_from(v).ok())
    }

    pub fn is_media(&self) -> bool {
        matches!(
            self.typed(),
            Some(MessageItemType::Image | MessageItemType::Voice | MessageItemType::File | MessageItemType::Video)
        )
    }
}

// ── Message ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Message {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete_time_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_type: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_state: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_list: Option<Vec<MessageItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_token: Option<String>,
}

impl Message {
    /// Extract text body from the first text item.
    pub fn text(&self) -> Option<&str> {
        self.item_list.as_ref()?.iter().find_map(|item| {
            if item.typed() == Some(MessageItemType::Text) {
                item.text_item.as_ref()?.text.as_deref()
            } else if item.typed() == Some(MessageItemType::Voice) {
                item.voice_item.as_ref()?.text.as_deref()
            } else {
                None
            }
        })
    }

    /// Find the first media item in the message.
    pub fn media_item(&self) -> Option<&MessageItem> {
        self.item_list
            .as_ref()?
            .iter()
            .find(|item| item.is_media())
    }
}

// ── GetUpdates ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct GetUpdatesRequest {
    pub get_updates_buf: String,
    pub base_info: BaseInfo,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GetUpdatesResponse {
    pub ret: Option<i32>,
    pub errcode: Option<i32>,
    pub errmsg: Option<String>,
    #[serde(default)]
    pub msgs: Option<Vec<Message>>,
    pub get_updates_buf: Option<String>,
    pub longpolling_timeout_ms: Option<u64>,
}

// ── SendMessage ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct SendMessageRequest {
    pub msg: Message,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_info: Option<BaseInfo>,
}

// ── GetUploadUrl ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct GetUploadUrlRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filekey: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rawsize: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rawfilemd5: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filesize: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_rawsize: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_rawfilemd5: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_filesize: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_need_thumb: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aeskey: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_info: Option<BaseInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GetUploadUrlResponse {
    pub upload_param: Option<String>,
    pub thumb_upload_param: Option<String>,
}

// ── GetConfig ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct GetConfigRequest {
    pub ilink_user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_info: Option<BaseInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GetConfigResponse {
    pub ret: Option<i32>,
    pub errmsg: Option<String>,
    pub typing_ticket: Option<String>,
}

// ── SendTyping ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct SendTypingRequest {
    pub ilink_user_id: String,
    pub typing_ticket: String,
    pub status: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_info: Option<BaseInfo>,
}

// ── QR Login ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct QrCodeResponse {
    pub qrcode: String,
    pub qrcode_img_content: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QrStatusResponse {
    pub status: String,
    pub bot_token: Option<String>,
    pub ilink_bot_id: Option<String>,
    pub baseurl: Option<String>,
    pub ilink_user_id: Option<String>,
}
