use std::{collections::HashMap, fmt};

use crate::{MediaTime, Packet};

pub mod ass;
pub mod nal;
pub mod webvtt;

#[derive(Clone, Debug)]
pub struct AssCodec {
    pub header: String,
}

#[derive(Clone, Debug)]
pub struct WebVttCodec {
    pub header: String,
}

#[derive(Clone, Debug)]
pub enum SubtitleCodec {
    Ass(AssCodec),
    WebVtt(WebVttCodec),
}

/// Information about a piece of subtitle media
#[derive(Clone)]
pub struct SubtitleInfo {
    pub codec: SubtitleCodec,
}

impl fmt::Debug for SubtitleInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.codec {
            SubtitleCodec::Ass(a) => {
                write!(f, "{}", a.header)?;
            }
            SubtitleCodec::WebVtt(a) => {
                write!(f, "{}", a.header)?;
            }
        }

        Ok(())
    }
}

#[derive(Default)]
pub struct SubtitleDescription {
    styles: HashMap<String, TextStyle>,
}

#[derive(Default, Debug)]
pub struct TextStyle {
    font: Option<String>,
    primary_color: Option<u32>,
    secondary_color: Option<u32>,
    outline_color: Option<u32>,
    back_color: Option<u32>,
    bold: bool,
    italic: bool,
    underline: bool,
    strikeout: bool,
    scale_x: f32,
    scale_y: f32,
    spacing: i32,
    angle: i32,
    border_style: Option<i32>,
    outline: Option<i32>,
    shadow: Option<i32>,
    alignment: Option<i32>,
    margin_left: Option<i32>,
    margin_right: Option<i32>,
    margin_vertical: Option<i32>,
}

#[derive(Debug)]
pub struct TextCue {
    pub time: MediaTime,
    pub style: String,
    pub text: Vec<TextPart>,
}

#[derive(Eq, PartialEq, Debug)]
pub enum TextAlign {
    TopLeft,
    Top,
    TopRight,
    MidLeft,
    Mid,
    MidRight,
    BotLeft,
    Bot,
    BotRight,
}

#[derive(Eq, PartialEq, Debug)]
pub enum ColorType {
    Primary,
    Karaoke,
    Outline,
    Shadow,
}

#[derive(Debug, PartialEq)]
pub struct TextPosition(f32, f32);

#[derive(Eq, PartialEq, Debug)]
pub struct TextFill(ColorType, u32);

#[derive(Eq, PartialEq, Debug)]
pub struct TextAlpha(ColorType, u8);

#[derive(Debug)]
pub enum TextPart {
    Text(String),
    Italic(bool),
    Underline(bool),
    Strikeout(bool),
    Border(f32),
    FontSize(u32),
    Position(TextPosition),
    Fill(TextFill),
    Alpha(TextAlpha),
    LineBreak,
    SmartBreak,
}

#[derive(Clone)]
pub struct SubtitleDecoderMetadata {
    pub(crate) name: &'static str,
    create: fn() -> Box<dyn SubtitleDecoder>,
}

impl SubtitleDecoderMetadata {
    pub fn create(&self) -> Box<dyn SubtitleDecoder> {
        (self.create)()
    }
}

inventory::collect!(SubtitleDecoderMetadata);

pub trait SubtitleDecoder: Send + Sync {
    fn start(&mut self, info: &SubtitleInfo) -> anyhow::Result<()>;
    fn feed(&mut self, packet: Packet) -> anyhow::Result<()>;
    fn receive(&mut self) -> Option<TextCue>;
}

#[derive(Clone)]
pub struct SubtitleEncoderMetadata {
    pub(crate) name: &'static str,
    create: fn() -> Box<dyn SubtitleEncoder>,
}

impl SubtitleEncoderMetadata {
    pub fn create(&self) -> Box<dyn SubtitleEncoder> {
        (self.create)()
    }
}

inventory::collect!(SubtitleEncoderMetadata);

pub trait SubtitleEncoder: Send + Sync {
    fn start(&mut self, desc: SubtitleDescription) -> anyhow::Result<SubtitleInfo>;
    fn feed(&mut self, cue: TextCue) -> anyhow::Result<()>;
    fn receive(&mut self) -> Option<Packet>;
}
