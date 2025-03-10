use super::prelude::*;

/// https://www.deviantart.com/developers/oembed
#[derive(Debug)]
pub struct DeviantArtExtractor;

impl ExtractorFactory for DeviantArtExtractor {
    fn create(&self, _config: &Config) -> Result<Option<Box<dyn Extractor>>, ConfigError> {
        Ok(Some(Box::new(DeviantArtExtractor)))
    }
}

#[async_trait::async_trait]
impl Extractor for DeviantArtExtractor {
    fn matches(&self, url: &Url) -> bool {
        match url.domain() {
            Some(d) if d.ends_with("deviantart.com") && url.path().contains("/art/") => true,
            Some("sta.sh" | "fav.me") if !url.path().is_empty() => true,
            _ => false,
        }
    }

    #[instrument(skip_all)]
    async fn extract(
        &self,
        state: Arc<ServiceState>,
        url: Url,
        params: Params,
    ) -> Result<EmbedWithExpire, Error> {
        let canonical_url = {
            let mut origin = url.origin().ascii_serialization();
            origin += url.path();
            origin
        };

        let oembed_uri = format!("https://backend.deviantart.com/oembed?url={canonical_url}");

        let resp = state.client.get(oembed_uri).send().await?;
        let oembed = resp.json::<DeviantArtOEmbed>().await?;

        let mut embed = EmbedV1::default();

        if oembed.safety == "adult" {
            embed.flags |= EmbedFlags::ADULT;
        }

        if !oembed.tags.is_empty() {
            embed.description = Some({
                let mut description = String::new();
                let tags: Vec<_> = oembed
                    .tags
                    .split_terminator(',')
                    .take(16) // take BEFORE collect
                    .chain((oembed.tags.len() > 16).then_some("more"))
                    .map(|tag| heck::AsTitleCase(tag.trim()))
                    .collect();

                crate::util::format_list(&mut description, tags).unwrap();
                description.into()
            });
        }

        let extra = crate::parser::embed::parse_oembed_to_embed(&mut embed, oembed.basic);
        let max_age = extra.max_age;

        // don't allow HTML embeds
        embed.obj = None;

        // oEmbed provides name/url
        embed.provider.icon = Some(
            Box::<EmbedMedia>::default()
                .with_url("https://st.deviantart.net/eclipse/icons/da_favicon_v2.ico"),
        );

        // thumbnails are often unnecessary for DA
        if embed.has_fullsize_media() {
            embed.thumb = None;
        }

        embed.color = Some(0x05cc47);
        embed.url = Some(canonical_url.into());

        // 4-hour expire
        Ok(generic::finalize_embed(
            state,
            embed,
            max_age.or(Some(60 * 60 * 4)),
        ))
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct DeviantArtOEmbed {
    #[serde(flatten)]
    pub basic: OEmbed,

    #[serde(default)]
    pub safety: SmolStr,

    #[serde(default)]
    pub tags: String,
}
