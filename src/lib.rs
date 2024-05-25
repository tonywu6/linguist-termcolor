//! Query GitHub's [Linguist] library for colors for programming languages.
//!
//! [Linguist]: https://github.com/github-linguist/linguist
//!
//! <pre>$ linguist-termcolor for rust
//! <strong style="color: #dea584 !important">rgb #dea584</strong> <strong style="color: #dfaf87 !important">xterm 180</strong> rust</pre>
//!
//! You can query with language names or file extensions.
//!
//! ## xterm colors and color distances
//!
//! For finding the nearest xterm colors, a `--colors`/`-c` option is available for specifying
//! the [color model][color-model] or [color space][color-space] in which to calculate
//! color differences using [`color_art::distance_with`].
//!
//! [color-model]: https://en.wikipedia.org/wiki/Color_model
//! [color-space]: https://en.wikipedia.org/wiki/Color_space
//!
//! The default is [RGB], which may not actually yield the best result in terms of human perception.
//! For finding colors that "look" the closest, [CIELAB] is a reasonable choice; use it with `-c lab`.
//! See [`ColorSpace`] for available choices.
//!
//! For example, here are the different results for `"python"` using [RGB], [CMYK], and [CIELAB], respectively.
//!
//! [RGB]: https://en.wikipedia.org/wiki/RGB_color_model
//! [CIELAB]: https://en.wikipedia.org/wiki/CIELAB_color_space
//! [CMYK]: https://en.wikipedia.org/wiki/CMYK_color_model
//!
//! <pre>$ linguist-termcolor -c rgb for python
//! <strong style="color: #3572a5 !important">rgb #3572a5</strong> <strong style="color: #5f5faf !important">xterm 61</strong> rust</pre>
//!
//! <pre>$ linguist-termcolor -c cmyk for python
//! <strong style="color: #3572a5 !important">rgb #3572a5</strong> <strong style="color: #5f87af !important">xterm 67</strong> rust</pre>
//!
//! <pre>$ linguist-termcolor -c lab for python
//! <strong style="color: #3572a5 !important">rgb #3572a5</strong> <strong style="color: #005f87 !important">xterm 24</strong> rust</pre>

use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
};

use color_art::{distance_with, Color, ColorSpace};
use colored::Colorize;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize;

/// Find the color among `choices` having the smallest distance to `color`
/// using [color_art::distance_with].
///
/// Returns the index and the color.
fn find_nearest_color<'a, I>(
    color: &Color,
    choices: I,
    colors: ColorSpace,
) -> Option<(usize, &'a Color)>
where
    I: Iterator<Item = &'a Color>,
{
    choices
        .map(|c| (c, distance_with(c, color, colors)))
        .enumerate()
        .min_by(|(_, (_, d1)), (_, (_, d2))| d1.partial_cmp(d2).unwrap())
        .map(|(i, (c, _))| (i, c))
}

/// See <https://github.com/github-linguist/linguist>
pub struct Linguist(HashMap<String, LinguistLang>);

/// See <https://github.com/github-linguist/linguist/blob/master/lib/linguist/languages.yml>
#[derive(Debug, Deserialize)]
struct LinguistLang {
    /// color in hex
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    extensions: Vec<String>,
    #[serde(default)]
    aliases: Vec<String>,
}

impl<'de> Deserialize<'de> for Linguist {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let map = HashMap::<String, LinguistLang>::deserialize(deserializer)?;
        Ok(Self(
            map.into_iter()
                .map(|(k, v)| (k.to_ascii_lowercase(), v))
                .collect(),
        ))
    }
}

impl Linguist {
    pub fn new() -> anyhow::Result<Self> {
        let url =
            "https://raw.githubusercontent.com/github/linguist/master/lib/linguist/languages.yml";
        eprintln!("{}", format!("Fetching {}", url).dimmed());
        let res = reqwest::blocking::get(url)?.error_for_status()?;
        let map = serde_yaml::from_str(&res.text()?)?;
        Ok(map)
    }

    /// Build a rudimentary search index for the colors.
    pub fn colors(&self) -> anyhow::Result<ColorMap<'_>> {
        let colors = self
            .0
            .values()
            .map(|lang| {
                lang.color
                    .as_ref()
                    .and_then(|c| u32::from_str_radix(&c[1..], 16).ok())
            })
            .collect::<Vec<_>>();

        let mut map = HashMap::<_, Vec<(Cow<'_, str>, u32)>>::with_capacity(
            self.0
                .values()
                .map(|lang| 1 + lang.aliases.len() + lang.extensions.len())
                .sum::<usize>(),
        );

        self.0.iter().enumerate().for_each(|(idx, (name, lang))| {
            let Some(color) = colors[idx] else { return };

            let text = std::iter::once(name.as_str())
                .chain(lang.aliases.iter().map(String::as_str))
                .chain(lang.extensions.iter().map(String::as_str));

            text.for_each(|keyword| {
                tokenize(keyword).iter().copied().for_each(|word| {
                    let name = Cow::from(name.as_str());
                    let word = Cow::from(word);
                    map.entry(word).or_default().push((name, color));
                })
            });
        });

        Ok(ColorMap(map))
    }
}

pub struct ColorMap<'a>(HashMap<Cow<'a, str>, Vec<(Cow<'a, str>, u32)>>);

impl ColorMap<'_> {
    pub fn query(&self, query: &str) -> BTreeMap<Cow<'_, str>, TermColor> {
        tokenize(query)
            .iter()
            .copied()
            .flat_map(|word| self.0.get(word))
            .flatten()
            .cloned()
            .map(|(name, color)| (name, TermColor::from(Color::from_num(color).unwrap())))
            .collect::<BTreeMap<_, _>>()
    }
}

#[derive(Debug)]
pub struct TermColor(Color);

impl From<Color> for TermColor {
    fn from(value: Color) -> Self {
        Self(value)
    }
}

impl TermColor {
    pub fn print(&self, colors: ColorSpace) -> String {
        let color = self.0;
        let xterm = find_nearest_color(&self.0, XTERM_COLORS.iter(), colors).unwrap();

        fn with_color(color: &Color, text: &str) -> colored::ColoredString {
            text.truecolor(color.red(), color.green(), color.blue())
        }

        let color_text = with_color(&color, &format!("rgb {}", color.hex_full())).bold();
        let xterm_text = with_color(xterm.1, &format!("xterm {:<3}", xterm.0)).bold(); // <3

        format!("{} {}", color_text, xterm_text)
    }
}

/// See:
///
/// - <https://gist.github.com/jasonm23/2868981#file-xterm-256color-yaml>
/// - <https://commons.wikimedia.org/wiki/File:Xterm_256color_chart.svg>
static XTERM_COLORS: Lazy<Vec<Color>> = Lazy::new(|| {
    let colors: &[u32; 256] = &[
        0x000000, 0x800000, 0x008000, 0x808000, 0x000080, 0x800080, 0x008080, 0xc0c0c0, 0x808080,
        0xff0000, 0x00ff00, 0xffff00, 0x0000ff, 0xff00ff, 0x00ffff, 0xffffff, 0x000000, 0x00005f,
        0x000087, 0x0000af, 0x0000d7, 0x0000ff, 0x005f00, 0x005f5f, 0x005f87, 0x005faf, 0x005fd7,
        0x005fff, 0x008700, 0x00875f, 0x008787, 0x0087af, 0x0087d7, 0x0087ff, 0x00af00, 0x00af5f,
        0x00af87, 0x00afaf, 0x00afd7, 0x00afff, 0x00d700, 0x00d75f, 0x00d787, 0x00d7af, 0x00d7d7,
        0x00d7ff, 0x00ff00, 0x00ff5f, 0x00ff87, 0x00ffaf, 0x00ffd7, 0x00ffff, 0x5f0000, 0x5f005f,
        0x5f0087, 0x5f00af, 0x5f00d7, 0x5f00ff, 0x5f5f00, 0x5f5f5f, 0x5f5f87, 0x5f5faf, 0x5f5fd7,
        0x5f5fff, 0x5f8700, 0x5f875f, 0x5f8787, 0x5f87af, 0x5f87d7, 0x5f87ff, 0x5faf00, 0x5faf5f,
        0x5faf87, 0x5fafaf, 0x5fafd7, 0x5fafff, 0x5fd700, 0x5fd75f, 0x5fd787, 0x5fd7af, 0x5fd7d7,
        0x5fd7ff, 0x5fff00, 0x5fff5f, 0x5fff87, 0x5fffaf, 0x5fffd7, 0x5fffff, 0x870000, 0x87005f,
        0x870087, 0x8700af, 0x8700d7, 0x8700ff, 0x875f00, 0x875f5f, 0x875f87, 0x875faf, 0x875fd7,
        0x875fff, 0x878700, 0x87875f, 0x878787, 0x8787af, 0x8787d7, 0x8787ff, 0x87af00, 0x87af5f,
        0x87af87, 0x87afaf, 0x87afd7, 0x87afff, 0x87d700, 0x87d75f, 0x87d787, 0x87d7af, 0x87d7d7,
        0x87d7ff, 0x87ff00, 0x87ff5f, 0x87ff87, 0x87ffaf, 0x87ffd7, 0x87ffff, 0xaf0000, 0xaf005f,
        0xaf0087, 0xaf00af, 0xaf00d7, 0xaf00ff, 0xaf5f00, 0xaf5f5f, 0xaf5f87, 0xaf5faf, 0xaf5fd7,
        0xaf5fff, 0xaf8700, 0xaf875f, 0xaf8787, 0xaf87af, 0xaf87d7, 0xaf87ff, 0xafaf00, 0xafaf5f,
        0xafaf87, 0xafafaf, 0xafafd7, 0xafafff, 0xafd700, 0xafd75f, 0xafd787, 0xafd7af, 0xafd7d7,
        0xafd7ff, 0xafff00, 0xafff5f, 0xafff87, 0xafffaf, 0xafffd7, 0xafffff, 0xd70000, 0xd7005f,
        0xd70087, 0xd700af, 0xd700d7, 0xd700ff, 0xd75f00, 0xd75f5f, 0xd75f87, 0xd75faf, 0xd75fd7,
        0xd75fff, 0xd78700, 0xd7875f, 0xd78787, 0xd787af, 0xd787d7, 0xd787ff, 0xd7af00, 0xd7af5f,
        0xd7af87, 0xd7afaf, 0xd7afd7, 0xd7afff, 0xd7d700, 0xd7d75f, 0xd7d787, 0xd7d7af, 0xd7d7d7,
        0xd7d7ff, 0xd7ff00, 0xd7ff5f, 0xd7ff87, 0xd7ffaf, 0xd7ffd7, 0xd7ffff, 0xff0000, 0xff005f,
        0xff0087, 0xff00af, 0xff00d7, 0xff00ff, 0xff5f00, 0xff5f5f, 0xff5f87, 0xff5faf, 0xff5fd7,
        0xff5fff, 0xff8700, 0xff875f, 0xff8787, 0xff87af, 0xff87d7, 0xff87ff, 0xffaf00, 0xffaf5f,
        0xffaf87, 0xffafaf, 0xffafd7, 0xffafff, 0xffd700, 0xffd75f, 0xffd787, 0xffd7af, 0xffd7d7,
        0xffd7ff, 0xffff00, 0xffff5f, 0xffff87, 0xffffaf, 0xffffd7, 0xffffff, 0x080808, 0x121212,
        0x1c1c1c, 0x262626, 0x303030, 0x3a3a3a, 0x444444, 0x4e4e4e, 0x585858, 0x626262, 0x6c6c6c,
        0x767676, 0x808080, 0x8a8a8a, 0x949494, 0x9e9e9e, 0xa8a8a8, 0xb2b2b2, 0xbcbcbc, 0xc6c6c6,
        0xd0d0d0, 0xdadada, 0xe4e4e4, 0xeeeeee,
    ];
    colors
        .iter()
        .map(|c| Color::from_num(*c).unwrap())
        .collect()
});

fn tokenize(text: &str) -> Vec<&str> {
    RE_MATCH_WORDS.find_iter(text).map(|m| m.as_str()).collect()
}

static RE_MATCH_WORDS: Lazy<Regex> = Lazy::new(|| Regex::new(r"[\pL\pN+*_#-]+").unwrap());
