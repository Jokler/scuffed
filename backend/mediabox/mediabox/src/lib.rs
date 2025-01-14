#![feature(str_split_as_str)] // used by ASS decoder
#![allow(unused_variables)]
#![allow(dead_code)]

use anyhow::Context;
use codec::{
    CodecDescription, Decoded, Decoder, DecoderMetadata, Encoder, EncoderMetadata,
    SubtitleDescription,
};
use std::{collections::HashMap, fmt};

pub mod media;
pub mod span;

pub mod codec;
pub mod format;
pub mod io;

pub use media::*;
pub use span::Span;

use format::{DemuxerMetadata, ProbeResult};
use io::Io;

pub fn find_decoder_for_track(track: &Track) -> anyhow::Result<Box<dyn Decoder>> {
    let mut decoder = inventory::iter::<DecoderMetadata>
        .into_iter()
        .find(|m| m.name == track.info.name)
        .map(|m| m.create())
        .ok_or_else(|| anyhow::anyhow!("No decoder found for {:?}", track.info.name))?;

    decoder.start(&track.info)?;

    Ok(decoder)
}

pub fn find_encoder_with_params(name: &str, info: &MediaInfo) -> anyhow::Result<Box<dyn Encoder>> {
    let mut encoder = inventory::iter::<EncoderMetadata>
        .into_iter()
        .find(|m| m.name == name)
        .map(|m| m.create());

    if let Some(ref mut encoder) = &mut encoder {
        encoder.start(CodecDescription::Subtitle(SubtitleDescription::default()))?;
    }

    encoder.ok_or_else(|| anyhow::anyhow!("No encoder found for name {name:?}"))
}

fn find_demuxer(data: &[u8]) -> Option<DemuxerMetadata> {
    inventory::iter::<DemuxerMetadata>
        .into_iter()
        .map(|m| (m, m.probe(data)))
        .reduce(|accum, m| if accum.1 >= m.1 { accum } else { m })
        .and_then(|(meta, result)| {
            if result != ProbeResult::Unsure {
                Some(meta.clone())
            } else {
                None
            }
        })
}

pub async fn probe(io: &mut Io) -> anyhow::Result<DemuxerMetadata> {
    let data = io
        .read_probe()
        .await
        .context("Failed to probe I/O for data")?;

    find_demuxer(data).ok_or_else(|| anyhow::anyhow!("Failed to find a demuxer"))
}

pub enum Transcode {
    Subtitles {
        decoder: Box<dyn Decoder>,
        encoder: Box<dyn Encoder>,
    },
}

pub struct PacketTranscoder {
    mapping: HashMap<u32, Transcode>,
}

impl PacketTranscoder {
    pub fn new(mapping: HashMap<u32, Transcode>) -> Self {
        PacketTranscoder { mapping }
    }
}

impl PacketTranscoder {
    // TODO: implement some queue and sort output on DTS
    pub async fn process<F: FnMut(Packet) + Send + 'static>(
        &mut self,
        pkt: Packet,
        mut func: F,
    ) -> anyhow::Result<()> {
        let track_id = pkt.track.id;
        let mut transcoding = if let Some(transcoding) = self.mapping.remove(&track_id) {
            transcoding
        } else {
            func(pkt);
            return Ok(());
        };

        let transcoding = tokio::task::spawn_blocking::<_, anyhow::Result<_>>(move || {
            process_transcode(pkt, track_id, &mut transcoding, func)?;

            Ok(transcoding)
        })
        .await??;

        self.mapping.insert(track_id, transcoding);

        Ok(())
    }
}

fn process_transcode<F: FnMut(Packet) + Send + 'static>(
    pkt: Packet,
    track_id: u32,
    transcoding: &mut Transcode,
    mut func: F,
) -> anyhow::Result<()> {
    match transcoding {
        Transcode::Subtitles {
            ref mut decoder,
            ref mut encoder,
        } => {
            decoder.feed(pkt)?;

            while let Some(decoded) = decoder.receive() {
                encoder.feed(decoded)?;

                while let Some(mut pkt) = encoder.receive() {
                    pkt.track.id = track_id;

                    func(pkt);
                }
            }
        }
    }

    Ok(())
}

#[derive(Copy, Clone)]
pub struct Fraction {
    pub numerator: u32,
    pub denominator: u32,
}

impl Fraction {
    pub const fn new(numerator: u32, denominator: u32) -> Self {
        Fraction {
            numerator,
            denominator,
        }
    }

    pub fn simplify(&self) -> Fraction {
        use gcd::Gcd;

        let divisor = self.numerator.gcd(self.denominator);

        Fraction::new(self.numerator / divisor, self.denominator / divisor)
    }

    pub fn decimal(&self) -> f32 {
        self.numerator as f32 / self.denominator as f32
    }
}

impl fmt::Display for Fraction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}/{}", self.numerator, self.denominator)
    }
}

impl fmt::Debug for Fraction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}
