use std::sync::LazyLock;

use pumpkin_config::{BASIC_CONFIG, advanced_config};
use pumpkin_protocol::{
    ConnectionState, KnownPack, Label, Link, LinkType,
    java::client::{
        config::{CConfigAddResourcePack, CConfigServerLinks, CKnownPacks, CUpdateTags},
        login::{CLoginSuccess, CSetCompression},
    },
    java::server::login::{
        SEncryptionResponse, SLoginCookieResponse, SLoginPluginResponse, SLoginStart,
    },
};
use pumpkin_util::text::TextComponent;
use uuid::Uuid;

use crate::{
    net::{
        GameProfile,
        authentication::{self, AuthError},
        is_valid_player_name,
        java::JavaClient,
        offline_uuid,
        proxy::{bungeecord, velocity},
    },
    server::Server,
};

static LINKS: LazyLock<Vec<Link>> = LazyLock::new(|| {
    let mut links: Vec<Link> = Vec::new();

    let bug_report = &advanced_config().server_links.bug_report;
    if !bug_report.is_empty() {
        links.push(Link::new(Label::BuiltIn(LinkType::BugReport), bug_report));
    }

    let support = &advanced_config().server_links.support;
    if !support.is_empty() {
        links.push(Link::new(Label::BuiltIn(LinkType::Support), support));
    }

    let status = &advanced_config().server_links.status;
    if !status.is_empty() {
        links.push(Link::new(Label::BuiltIn(LinkType::Status), status));
    }

    let feedback = &advanced_config().server_links.feedback;
    if !feedback.is_empty() {
        links.push(Link::new(Label::BuiltIn(LinkType::Feedback), feedback));
    }

    let community = &advanced_config().server_links.community;
    if !community.is_empty() {
        links.push(Link::new(Label::BuiltIn(LinkType::Community), community));
    }

    let website = &advanced_config().server_links.website;
    if !website.is_empty() {
        links.push(Link::new(Label::BuiltIn(LinkType::Website), website));
    }

    let forums = &advanced_config().server_links.forums;
    if !forums.is_empty() {
        links.push(Link::new(Label::BuiltIn(LinkType::Forums), forums));
    }

    let news = &advanced_config().server_links.news;
    if !news.is_empty() {
        links.push(Link::new(Label::BuiltIn(LinkType::News), news));
    }

    let announcements = &advanced_config().server_links.announcements;
    if !announcements.is_empty() {
        links.push(Link::new(
            Label::BuiltIn(LinkType::Announcements),
            announcements,
        ));
    }

    for (key, value) in &advanced_config().server_links.custom {
        links.push(Link::new(
            Label::TextComponent(TextComponent::text(key).into()),
            value,
        ));
    }
    links
});

impl JavaClient {
    pub async fn handle_login_start(&self, server: &Server, login_start: SLoginStart) {
        log::debug!("login start");

        // Don't allow new logons when the server is full.
        // If `max_players` is set to zero, then there is no max player count enforced.
        // TODO: If client is an operator or has otherwise suitable elevated permissions, allow the client to bypass this requirement.
        let max_players = BASIC_CONFIG.max_players;
        if max_players > 0 && server.get_player_count().await >= max_players as usize {
            self.kick(TextComponent::translate(
                "multiplayer.disconnect.server_full",
                [],
            ))
            .await;
            return;
        }

        if !is_valid_player_name(&login_start.name) {
            self.kick(TextComponent::text("Invalid characters in username"))
                .await;
            return;
        }
        // Default game profile, when no online mode
        // TODO: Make offline UUID
        let mut gameprofile = self.gameprofile.lock().await;
        let proxy = &advanced_config().networking.proxy;
        if proxy.enabled {
            if proxy.velocity.enabled {
                velocity::velocity_login(self).await;
            } else if proxy.bungeecord.enabled {
                match bungeecord::bungeecord_login(
                    &self.address,
                    &self.server_address.lock().await,
                    login_start.name,
                )
                .await
                {
                    Ok((_ip, profile)) => {
                        // self.address.lock() = ip;
                        self.finish_login(&profile).await;
                        *gameprofile = Some(profile);
                    }
                    Err(error) => self.kick(TextComponent::text(error.to_string())).await,
                }
            }
        } else {
            let id = if BASIC_CONFIG.online_mode {
                login_start.uuid
            } else {
                offline_uuid(&login_start.name).expect("This is very not safe and bad")
            };

            let profile = GameProfile {
                id,
                name: login_start.name,
                properties: vec![],
                profile_actions: None,
            };

            if advanced_config().networking.packet_compression.enabled {
                self.enable_compression().await;
            }

            if BASIC_CONFIG.encryption {
                let verify_token: [u8; 4] = rand::random();
                // Wait until we have sent the encryption packet to the client
                self.send_packet_now(
                    &server.encryption_request(&verify_token, BASIC_CONFIG.online_mode),
                )
                .await;
            } else {
                self.finish_login(&profile).await;
            }

            *gameprofile = Some(profile);
        }
    }

    pub async fn handle_encryption_response(
        &self,
        server: &Server,
        encryption_response: SEncryptionResponse,
    ) {
        log::debug!("Handling encryption");
        let shared_secret = server.decrypt(&encryption_response.shared_secret).unwrap();

        if let Err(error) = self.set_encryption(&shared_secret).await {
            self.kick(TextComponent::text(error.to_string())).await;
            return;
        }

        let mut gameprofile = self.gameprofile.lock().await;

        let Some(profile) = gameprofile.as_mut() else {
            self.kick(TextComponent::text("No `GameProfile`")).await;
            return;
        };

        if BASIC_CONFIG.online_mode {
            // Online mode auth
            match self
                .authenticate(server, &shared_secret, &profile.name)
                .await
            {
                Ok(new_profile) => *profile = new_profile,
                Err(error) => {
                    self.kick(match error {
                        AuthError::FailedResponse => {
                            TextComponent::translate("multiplayer.disconnect.authservers_down", [])
                        }
                        AuthError::UnverifiedUsername => TextComponent::translate(
                            "multiplayer.disconnect.unverified_username",
                            [],
                        ),
                        e => TextComponent::text(e.to_string()),
                    })
                    .await;
                }
            }
        }

        // Don't allow duplicate UUIDs
        if let Some(online_player) = &server.get_player_by_uuid(profile.id).await {
            log::debug!(
                "Player (IP '{}', username '{}') tried to log in with the same UUID ('{}') as an online player (username '{}')",
                &self.address.lock().await,
                &profile.name,
                &profile.id,
                &online_player.gameprofile.name
            );
            self.kick(TextComponent::translate(
                "multiplayer.disconnect.duplicate_login",
                [],
            ))
            .await;
            return;
        }

        // Don't allow a duplicate username
        if let Some(online_player) = &server.get_player_by_name(&profile.name).await {
            log::debug!(
                "A player (IP '{}', attempted username '{}') tried to log in with the same username as an online player (UUID '{}', username '{}')",
                &self.address.lock().await,
                &profile.name,
                &profile.id,
                &online_player.gameprofile.name
            );
            self.kick(TextComponent::translate(
                "multiplayer.disconnect.duplicate_login",
                [],
            ))
            .await;
            return;
        }

        self.finish_login(profile).await;
    }

    async fn enable_compression(&self) {
        let compression = advanced_config().networking.packet_compression.info.clone();
        // We want to wait until we have sent the compression packet to the client
        self.send_packet_now(&CSetCompression::new(
            compression.threshold.try_into().unwrap(),
        ))
        .await;
        self.set_compression(compression).await;
    }

    async fn finish_login(&self, profile: &GameProfile) {
        let packet = CLoginSuccess::new(&profile.id, &profile.name, &profile.properties);
        self.send_packet_now(&packet).await;
    }

    async fn authenticate(
        &self,
        server: &Server,
        shared_secret: &[u8],
        username: &str,
    ) -> Result<GameProfile, AuthError> {
        let hash = server.digest_secret(shared_secret);
        let ip = self.address.lock().await.ip();
        let profile = authentication::authenticate(username, &hash, &ip)?;

        // Check if the player should join
        if let Some(actions) = &profile.profile_actions {
            if advanced_config()
                .networking
                .authentication
                .player_profile
                .allow_banned_players
            {
                for allowed in &advanced_config()
                    .networking
                    .authentication
                    .player_profile
                    .allowed_actions
                {
                    if !actions.contains(allowed) {
                        return Err(AuthError::DisallowedAction);
                    }
                }
                if !actions.is_empty() {
                    return Err(AuthError::Banned);
                }
            } else if !actions.is_empty() {
                return Err(AuthError::Banned);
            }
        }
        // Validate textures
        for property in &profile.properties {
            authentication::validate_textures(
                property,
                &advanced_config().networking.authentication.textures,
            )
            .map_err(AuthError::TextureError)?;
        }
        Ok(profile)
    }

    pub fn handle_login_cookie_response(&self, packet: &SLoginCookieResponse) {
        // TODO: allow plugins to access this
        log::debug!(
            "Received cookie_response[login]: key: \"{}\", payload_length: \"{:?}\"",
            packet.key,
            packet.payload.as_ref().map(|p| p.len())
        );
    }
    pub async fn handle_plugin_response(&self, plugin_response: SLoginPluginResponse) {
        log::debug!("Handling plugin");
        let velocity_config = &advanced_config().networking.proxy.velocity;
        if velocity_config.enabled {
            let mut address = self.address.lock().await;
            match velocity::receive_velocity_plugin_response(
                address.port(),
                velocity_config,
                plugin_response,
            ) {
                Ok((profile, new_address)) => {
                    self.finish_login(&profile).await;
                    *self.gameprofile.lock().await = Some(profile);
                    *address = new_address;
                }
                Err(error) => self.kick(TextComponent::text(error.to_string())).await,
            }
        }
    }

    pub async fn handle_login_acknowledged(&self, server: &Server) {
        log::debug!("Handling login acknowledgement");
        self.connection_state.store(ConnectionState::Config);
        self.send_packet_now(&server.get_branding()).await;

        if advanced_config().server_links.enabled {
            self.send_packet_now(&CConfigServerLinks::new(&LINKS)).await;
        }

        // Send tags.
        // TODO: Is this the right place to send them?

        self.send_packet_now(&CUpdateTags::new(&[
            pumpkin_data::tag::RegistryKey::Block,
            pumpkin_data::tag::RegistryKey::Fluid,
        ]))
        .await;

        let resource_config = &advanced_config().resource_pack;
        if resource_config.enabled {
            let uuid = Uuid::new_v3(&uuid::Uuid::NAMESPACE_DNS, resource_config.url.as_bytes());
            let resource_pack = CConfigAddResourcePack::new(
                &uuid,
                &resource_config.url,
                &resource_config.sha1,
                resource_config.force,
                if resource_config.prompt_message.is_empty() {
                    None
                } else {
                    Some(TextComponent::text(&resource_config.prompt_message))
                },
            );

            self.send_packet_now(&resource_pack).await;
        } else {
            // This will be invoked by our resource pack handler in the case of the above branch.
            self.send_known_packs().await;
        }
        log::debug!("login acknowledged");
    }

    /// Send the known data packs to the client.
    pub async fn send_known_packs(&self) {
        self.send_packet_now(&CKnownPacks::new(&[KnownPack {
            namespace: "minecraft",
            id: "core",
            version: "1.21",
        }]))
        .await;
    }
}
