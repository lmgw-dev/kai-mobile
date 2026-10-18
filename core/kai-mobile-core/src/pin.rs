//! The lmgw source this build is pinned to (`LMGW_PIN`, read by build.rs).

/// The three lines of `LMGW_PIN`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LmgwPin {
    /// The commit in lmgw's development history.
    pub commit: String,
    /// The published lmgw release with the pinned tree, or `unpublished`.
    pub tag: String,
    /// The commit's tree, which the container build stages lmgw's crates from.
    pub tree: String,
}

impl LmgwPin {
    /// The pin this library was built with.
    pub fn built_in() -> Self {
        Self {
            commit: env!("KAI_LMGW_COMMIT").to_owned(),
            tag: env!("KAI_LMGW_TAG").to_owned(),
            tree: env!("KAI_LMGW_TREE").to_owned(),
        }
    }

    /// Reads `LMGW_PIN`'s text: `#` lines are comments, every field is required, the commit and
    /// the tree are 40 hex digits and the tag is `unpublished` or `vX.Y.Z`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let field = |key: &str| -> Result<String, String> {
            text.lines()
                .filter(|l| !l.trim_start().starts_with('#'))
                .find_map(|l| {
                    let mut words = l.split_whitespace();
                    if words.next() == Some(key) {
                        words.next().map(str::to_owned)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| format!("no {key}"))
        };
        let hex40 = |what: &str, v: String| -> Result<String, String> {
            if v.len() == 40 && v.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
                Ok(v)
            } else {
                Err(format!("{what} is not 40 hex digits"))
            }
        };
        let tag = field("lmgw_tag")?;
        let release = tag.strip_prefix('v').is_some_and(|v| {
            let parts: Vec<&str> = v.split('.').collect();
            parts.len() == 3
                && parts
                    .iter()
                    .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        });
        if tag != "unpublished" && !release {
            return Err("lmgw_tag is neither `unpublished` nor vX.Y.Z".to_owned());
        }
        Ok(Self {
            commit: hex40("lmgw_commit", field("lmgw_commit")?)?,
            tag,
            tree: hex40("lmgw_tree", field("lmgw_tree")?)?,
        })
    }

    /// The commit's first seven digits, as the About page shows it.
    pub fn short_commit(&self) -> &str {
        &self.commit[..7.min(self.commit.len())]
    }

    /// Whether a published lmgw release has the pinned tree.
    pub fn is_published(&self) -> bool {
        self.tag != "unpublished"
    }
}
