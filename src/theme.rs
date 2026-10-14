use gpui::{Global, Hsla, SharedString, rgb, rgba};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    Dark,
    Light,
}

impl Appearance {
    pub fn toggled(self) -> Self {
        match self {
            Appearance::Dark => Appearance::Light,
            Appearance::Light => Appearance::Dark,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Syntax {
    pub heading: Hsla,
    pub marker: Hsla,
    pub code: Hsla,
    pub link: Hsla,
    pub quote: Hsla,
    pub string: Hsla,
    pub keyword: Hsla,
    pub comment: Hsla,
    pub number: Hsla,
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub appearance: Appearance,
    pub background: Hsla,
    pub shell: Hsla,
    pub raised: Hsla,
    pub card: Hsla,
    pub hover: Hsla,
    pub active: Hsla,
    pub border: Hsla,
    pub border_strong: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub faint: Hsla,
    pub accent: Hsla,
    pub on_accent: Hsla,
    pub selection: Hsla,
    pub cursor: Hsla,
    pub current_line: Hsla,
    pub danger: Hsla,
    pub warning: Hsla,
    pub success: Hsla,
    pub syntax: Syntax,
    pub sans: SharedString,
    pub mono: SharedString,
}

impl Global for Theme {}

impl Theme {
    pub fn new(appearance: Appearance, sans: SharedString, mono: SharedString) -> Self {
        match appearance {
            Appearance::Dark => Self {
                appearance,
                background: rgb(0x08080a).into(),
                shell: rgb(0x0e0e11).into(),
                raised: rgb(0x18181c).into(),
                card: rgb(0x111114).into(),
                hover: rgba(0xffffff12).into(),
                active: rgba(0x8b7cf62e).into(),
                border: rgba(0xffffff14).into(),
                border_strong: rgba(0xffffff26).into(),
                text: rgb(0xe8e8ea).into(),
                muted: rgb(0xa9a9ae).into(),
                faint: rgb(0x6f6f76).into(),
                accent: rgb(0x8b7cf6).into(),
                on_accent: rgb(0xffffff).into(),
                selection: rgba(0x8b7cf64d).into(),
                cursor: rgb(0xb3a7ff).into(),
                current_line: rgba(0xffffff07).into(),
                danger: rgb(0xf87171).into(),
                warning: rgb(0xfacc15).into(),
                success: rgb(0x34d399).into(),
                syntax: Syntax {
                    heading: rgb(0xc4b5fd).into(),
                    marker: rgb(0x8b7cf6).into(),
                    code: rgb(0x34d399).into(),
                    link: rgb(0x60a5fa).into(),
                    quote: rgb(0x92929a).into(),
                    string: rgb(0xfbbf24).into(),
                    keyword: rgb(0xf472b6).into(),
                    comment: rgb(0x6f6f76).into(),
                    number: rgb(0x22d3ee).into(),
                },
                sans,
                mono,
            },
            Appearance::Light => Self {
                appearance,
                background: rgb(0xffffff).into(),
                shell: rgb(0xf5f5f7).into(),
                raised: rgb(0xebebef).into(),
                card: rgb(0xfafafb).into(),
                hover: rgba(0x0000000d).into(),
                active: rgba(0x6d5ce61a).into(),
                border: rgba(0x0000001a).into(),
                border_strong: rgba(0x00000030).into(),
                text: rgb(0x1f1f24).into(),
                muted: rgb(0x5f5f68).into(),
                faint: rgb(0x8e8e96).into(),
                accent: rgb(0x6d5ce6).into(),
                on_accent: rgb(0xffffff).into(),
                selection: rgba(0x6d5ce640).into(),
                cursor: rgb(0x5b48e0).into(),
                current_line: rgba(0x00000006).into(),
                danger: rgb(0xdc2626).into(),
                warning: rgb(0xb45309).into(),
                success: rgb(0x059669).into(),
                syntax: Syntax {
                    heading: rgb(0x5b48e0).into(),
                    marker: rgb(0x6d5ce6).into(),
                    code: rgb(0x047857).into(),
                    link: rgb(0x2563eb).into(),
                    quote: rgb(0x6b6b74).into(),
                    string: rgb(0xb45309).into(),
                    keyword: rgb(0xbe185d).into(),
                    comment: rgb(0x8e8e96).into(),
                    number: rgb(0x0e7490).into(),
                },
                sans,
                mono,
            },
        }
    }
}
