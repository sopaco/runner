use gpui::{font, px, App, AppContext, Context, Entity, Global, Task, TextSystem};

pub(crate) const BUNDLED_FONT: &str = "JetBrainsMono Nerd Font Mono";

#[derive(Default)]
pub(crate) struct FontFamilies {
    pub(crate) all: Vec<String>,
    pub(crate) monospace: Vec<String>,
}

pub(crate) struct FontFamilyCache {
    pub(crate) families: Option<FontFamilies>,
    _task: Task<()>,
}

struct GlobalFontFamilyCache(Entity<FontFamilyCache>);
impl Global for GlobalFontFamilyCache {}

impl FontFamilyCache {
    pub(crate) fn global(cx: &mut App) -> Entity<Self> {
        if let Some(cache) = cx.try_global::<GlobalFontFamilyCache>() {
            return cache.0.clone();
        }
        let cache = cx.new(|cx: &mut Context<Self>| {
            let text_system = cx.text_system().clone();
            let load = cx.background_spawn(async move {
                let all = text_system.all_font_names();
                let monospace = all
                    .iter()
                    .filter(|name| name.as_str() != BUNDLED_FONT)
                    .filter(|name| is_monospace(&text_system, name))
                    .cloned()
                    .collect();
                FontFamilies { all, monospace }
            });
            Self {
                families: None,
                _task: cx.spawn(async move |cache, cx| {
                    let families = load.await;
                    let _ = cache.update(cx, |cache, cx| {
                        cache.families = Some(families);
                        cx.notify();
                        cx.refresh_windows();
                    });
                }),
            }
        });
        cx.set_global(GlobalFontFamilyCache(cache.clone()));
        cache
    }

    pub(crate) fn loaded(cx: &App) -> Option<&FontFamilies> {
        cx.try_global::<GlobalFontFamilyCache>()?
            .0
            .read(cx)
            .families
            .as_ref()
    }
}

fn is_monospace(text_system: &TextSystem, family: &str) -> bool {
    let id = text_system.resolve_font(&font(family.to_owned()));
    matching_advances(|ch| {
        text_system
            .advance(id, px(16.), ch)
            .ok()
            .map(|size| f32::from(size.width))
    })
}

fn matching_advances(advance: impl Fn(char) -> Option<f32>) -> bool {
    match ['i', 'W', 'm'].map(advance) {
        [Some(i), Some(w), Some(m)] => i > 0. && (i - w).abs() < 0.01 && (i - m).abs() < 0.01,
        _ => false,
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use font_kit::font::Font;
    use std::sync::Arc;

    #[test]
    fn monospace_filter_checks_real_glyph_advances() {
        let bundled =
            Font::from_bytes(Arc::new(crate::assets::JETBRAINS_MONO_FONTS[0].to_vec()), 0).unwrap();
        let inter = Font::from_bytes(Arc::new(crate::assets::INTER_FONTS[0].to_vec()), 0).unwrap();
        let accepts = |font: &Font| {
            matching_advances(|ch| {
                font.advance(font.glyph_for_char(ch)?)
                    .ok()
                    .map(|advance| advance.x())
            })
        };
        assert!(accepts(&bundled));
        assert!(!accepts(&inter));
    }
}
