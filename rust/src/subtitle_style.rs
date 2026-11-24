//! Project-wide caption styling shared by preview and export.

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubtitleStyle {
    pub font_family: String,
    pub font_size: u16,
    pub text_color: [u8; 3],
    pub outline_color: [u8; 3],
    pub outline_width: u8,
}

impl Default for SubtitleStyle {
    fn default() -> Self {
        Self {
            font_family: "Arial".into(),
            font_size: 24,
            text_color: [255, 255, 255],
            outline_color: [16, 16, 16],
            outline_width: 2,
        }
    }
}

impl SubtitleStyle {
    pub fn validate(&self) -> Result<()> {
        let family = &self.font_family;
        ensure!(
            !family.is_empty()
                && family.trim() == family
                && family.chars().count() <= 128
                && !family
                    .chars()
                    .any(|ch| ch.is_control() || matches!(ch, ',' | '{' | '}' | '\\')),
            "有効なフォント名を入力してください"
        );
        ensure!(
            (8..=96).contains(&self.font_size),
            "字幕の文字サイズは 8〜96 で指定してください"
        );
        ensure!(
            self.outline_width <= 12,
            "字幕の縁取りは 0〜12 で指定してください"
        );
        Ok(())
    }

    pub(crate) fn is_default(&self) -> bool {
        *self == Self::default()
    }
}
