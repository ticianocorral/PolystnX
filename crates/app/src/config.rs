//! `polystnx.cfg`: run-ahead default, start-fullscreen, and keyboard binds —
//! the same file the in-app settings screen (`O` on the shelf) edits and
//! saves. TOML syntax under the hood (same as before); only the name and
//! location changed — it now sits in the app's root (`polystnx_app::dirs`,
//! next to the executable on Windows/Linux, `~/Documents/PolystnX` on
//! macOS), not under `~/.config` (plan: app portátil).
//!
//! Lookup order: `--config PATH`, then `$PSX_XPERIENCE_CONFIG`, then
//! `polystnx_app::dirs::config_path()`. When the last one is used and is
//! missing, a default file is written so the user has something to edit.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use polystnx_platform::{KeyMap, PadButton, PadMap};
use serde::Deserialize;

pub struct Config {
    pub fullscreen: bool,
    /// Check GitHub for a newer app release, and the buildbot for a fresher
    /// PCSX Rearmed core, once at startup (plan revision) — a settings-screen
    /// toggle, on by default. Either check that finds something newer shows
    /// a one-time notice on the idle screen; a network failure just means no
    /// notice, never an error.
    pub check_updates_on_start: bool,
    /// Silencia o áudio do jogo (só o som de ambiente do leitor toca).
    pub mute_game: bool,
    /// Cheater Fish: ao ligar o console com um disco que tem cheats, mostra
    /// primeiro a tela de cheats (estilo GameShark) antes do jogo.
    pub cheater_fish: bool,
    /// Volume da TV, 0..=16 (os botões VOL- / VOL+ do queixo).
    pub tv_volume: u8,
    /// Menu de imagem da TV: brilho, contraste, cor e nitidez (0..=100).
    pub tv_picture: [u8; 4],
    /// LEGADO: o suporte ao CONTROLE 2 saiu do painel (plan revision,
    /// "no momento") — o campo morre na leitura, sem efeito.
    /// A BIOS padrão (nome do arquivo em `bios/`); `None` = o core varre a
    /// pasta como sempre fez.
    pub bios_default: Option<String>,
    /// RetroAchievements account (plan: `docs/plano-retroachievements.md`,
    /// fase 1) — the username and the web-API token the user generates on
    /// retroachievements.org (Settings → Web API). Both empty (the default)
    /// keeps every RA feature off: no network call, no UI beyond the
    /// settings rows themselves.
    pub ra_user: String,
    pub ra_token: String,
    /// Hardcore mode for achievements (default on once an account exists) —
    /// cheats/savestates/run-ahead lock during play. Meaningful from fase 3
    /// on; persisted now so the setting doesn't move later.
    pub ra_hardcore: bool,
    /// O card encaixado em cada slot do console (plan revision: "deve sempre
    /// utilizar o card que está inserido no console") — caminhos para os
    /// `.mcr` da pasta memcards, persistidos para sobreviver ao app. `None`
    /// = slot vazio (o jogo vê um card vazio efêmero e não salva em nada).
    pub card_slot1: Option<String>,
    pub card_slot2: Option<String>,
    /// Connect login token (plan revision — a web API key não serve mais a
    /// lógica das conquistas: o campo `MemAddr` da rota velha chega
    /// hashado). Nasce do `r=login2` com usuário+senha na tela de
    /// configurações; a senha em si nunca é gravada. `None` = conta ainda
    /// não conectada — a estante não identifica nada até conectar.
    pub ra_connect: Option<String>,
    pub keymap: KeyMap,
    /// Gamepad-button layout (`[gamepad]`): which SDL gamepad button drives
    /// each PS1 button. Rebindable in the settings screen alongside the
    /// keyboard — clones and native-driver paths can land a press in the
    /// wrong slot, so the layout is the user's to fix.
    pub padmap: PadMap,
    /// Where it was read from (or freshly written), for logging and for
    /// `save()`.
    pub source: Option<PathBuf>,
}

#[derive(Deserialize, Default)]
struct Raw {
    #[serde(default)]
    fullscreen: Option<bool>,
    #[serde(default)]
    check_updates_on_start: Option<bool>,
    mute_game: Option<bool>,
    #[serde(default)]
    cheater_fish: Option<bool>,
    #[serde(default)]
    tv_volume: Option<u8>,
    #[serde(default)]
    tv_picture: Option<[u8; 4]>,
    card_slot1: Option<String>,
    card_slot2: Option<String>,
    bios_default: Option<String>,
    #[serde(default)]
    ra_user: Option<String>,
    #[serde(default)]
    ra_token: Option<String>,
    #[serde(default)]
    ra_hardcore: Option<bool>,
    #[serde(default)]
    ra_connect: Option<String>,
    #[serde(default)]
    keyboard: BTreeMap<String, String>,
    #[serde(default)]
    gamepad: BTreeMap<String, String>,
}

impl Config {
    pub fn load(explicit: Option<&Path>) -> Result<Self> {
        let (path, required) = match explicit {
            Some(p) => (Some(p.to_path_buf()), true),
            None => match std::env::var_os("PSX_XPERIENCE_CONFIG") {
                Some(p) => (Some(PathBuf::from(p)), true),
                None => (Some(crate::dirs::config_path()), false),
            },
        };
        Self::load_at(path, required)
    }

    /// O miolo do [`Config::load`]: `required` = o caminho veio do jogador
    /// (`--config`/`PSX_XPERIENCE_CONFIG`) e um erro nele é erro; o padrão
    /// se recupera sozinho.
    fn load_at(path: Option<PathBuf>, required: bool) -> Result<Self> {
        let mut cfg = Config {
            // Estados de PSX são grandes (MB) e o core é pesado — o
            // run-ahead de PSX nasce desligado (plano §3.1); medição na
            // Fase 1 decide se 1 volta.
            // Plan revision: "sempre abrir em fullscreen como padrao" — a
            // fresh install (no `polystnx.cfg` yet) starts fullscreen; the
            // settings screen's own toggle still turns it off from there.
            fullscreen: true,
            check_updates_on_start: true,
            mute_game: false,
            cheater_fish: true,
            tv_volume: 16,
            tv_picture: [50; 4],
            card_slot1: None,
            card_slot2: None,
            bios_default: None,
            ra_user: String::new(),
            ra_token: String::new(),
            ra_hardcore: true,
            ra_connect: None,
            keymap: KeyMap::defaults(),
            padmap: PadMap::defaults(),
            source: path.clone(),
        };

        let Some(path) = path else {
            return Ok(cfg);
        };

        // `fs::read` + UTF-8 à parte: um arquivo com bytes inválidos vira erro
        // de conteúdo (vai para `.bad` abaixo), não de leitura.
        match std::fs::read(&path) {
            Ok(bytes) => {
                tighten_permissions(&path);
                let parsed = String::from_utf8(bytes)
                    .map_err(|_| anyhow::anyhow!("{}: não é UTF-8 válido", path.display()))
                    .and_then(|text| parse_raw(&text, &path))
                    .and_then(|raw| cfg.apply(raw));
                if let Err(e) = parsed {
                    if required {
                        return Err(e);
                    }
                    // O cfg padrão (editado à mão e quebrado) não pode impedir o
                    // app de abrir: vai para `.bad` — guardado, não apagado —
                    // e o app sobe com os padrões gravados de novo.
                    let bad = path.with_extension("cfg.bad");
                    log::error!(
                        "{e:#} — o arquivo vai para {} e o app segue com os padrões",
                        bad.display()
                    );
                    std::fs::rename(&path, &bad)
                        .with_context(|| format!("renomeando {}", path.display()))?;
                    // O `.bad` leva o token do RA: um cfg editado à mão costuma
                    // voltar 0644 do editor, e o rename preserva o modo.
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let _ =
                            std::fs::set_permissions(&bad, std::fs::Permissions::from_mode(0o600));
                    }
                    return Self::load_at(Some(path), required);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && !required => {
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let _ = write_config_private(&path, &cfg.to_toml());
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                bail!("config not found: {}", path.display());
            }
            Err(e) => {
                return Err(e).with_context(|| format!("reading {}", path.display()));
            }
        }
        Ok(cfg)
    }

    fn apply(&mut self, raw: Raw) -> Result<()> {
        if let Some(f) = raw.fullscreen {
            self.fullscreen = f;
        }
        if let Some(c) = raw.check_updates_on_start {
            self.check_updates_on_start = c;
        }
        if let Some(v) = raw.tv_volume {
            self.tv_volume = v.min(16);
        }
        if let Some(p) = raw.tv_picture {
            self.tv_picture = p.map(|v| v.min(100));
        }
        if let Some(c) = raw.cheater_fish {
            self.cheater_fish = c;
        }
        if let Some(m) = raw.mute_game {
            self.mute_game = m;
        }
        if let Some(c) = raw.card_slot1 {
            self.card_slot1 = if c.is_empty() { None } else { Some(c) };
        }
        if let Some(c) = raw.card_slot2 {
            self.card_slot2 = if c.is_empty() { None } else { Some(c) };
        }
        if let Some(b) = raw.bios_default.clone() {
            self.bios_default = Some(b);
        }
        if let Some(u) = raw.ra_user {
            self.ra_user = u;
        }
        if let Some(t) = raw.ra_token {
            self.ra_token = t;
        }
        if let Some(h) = raw.ra_hardcore {
            self.ra_hardcore = h;
        }
        // Connect token só chega pelo login da tela de configurações — mas
        // respeita um que tenha sido colado à mão no arquivo.
        if let Some(c) = raw.ra_connect {
            self.ra_connect = (!c.is_empty()).then_some(c);
        }
        for (action, key) in &raw.keyboard {
            let Ok(b) = action.parse::<PadButton>() else {
                // Not a gameplay button — either a typo, or (most likely for
                // anyone upgrading) one of the console/UI commands this
                // section used to rebind (eject, pause, save_state, ...)
                // before those became mouse/gamepad-only. Either way,
                // nothing left to bind it to; warn and move on rather than
                // refusing to start over a stale config line.
                log::warn!("[keyboard] {action}: not a bindable action any more, ignoring");
                continue;
            };
            // Uma tecla inválida perde só ela — não o arquivo inteiro (que
            // levaria junto o login do RA e os mapas dos controles).
            if let Err(e) = self.keymap.bind_pad(key, b) {
                log::warn!("[keyboard] {action}: {e} — ignorado");
            }
        }
        for (action, button) in &raw.gamepad {
            let Ok(b) = action.parse::<PadButton>() else {
                log::warn!("[gamepad] {action}: not a bindable action, ignoring");
                continue;
            };
            if button.is_empty() {
                // Written for an action whose button another action took —
                // nothing to bind here (the empty state re-emerges from the
                // other action's bind dropping the default).
                continue;
            }
            if let Err(e) = self.padmap.bind(button, b) {
                log::warn!("[gamepad] {action}: {e} — ignorado");
            }
        }
        Ok(())
    }

    /// Serialize the live config back to the `polystnx.cfg` text format.
    pub fn to_toml(&self) -> String {
        let mut s = String::from(
            "# PolystnX configuration.\n\
             # Edited by the in-app settings screen (O on the shelf) — hand edits\n\
             # survive a save, but comments outside a value don't.\n\
             # fullscreen: start in fullscreen.\n\
             # check_updates_on_start: look for a newer release/PCSX Rearmed core at launch.\n\
             # RetroAchievements: ra_user/ra_token from retroachievements.org\n\
             # (Settings -> Web API); empty = the whole feature stays off.\n\
             # ra_hardcore: no cheats/savestates while earning achievements.\n\
             # card_slot1/card_slot2: LEGADO (sem efeito) — os slots do\n\
             # console nascem vazios em todo jogo; inserir card é manual,\n\
             # pelo modal de MC no painel.\n\
             # [keyboard]: action = \"SDL key name\" (e.g. \"Left Shift\", \"F2\", \"]\").\n\
             # [gamepad]: action = \"SDL gamepad button name\" (e.g. \"south\",\n\
             # \"dpup\", \"leftshoulder\") — which pad button drives each action.\n\n",
        );
        s.push_str(&format!("fullscreen = {}\n", self.fullscreen));
        s.push_str(&format!(
            "check_updates_on_start = {}\n",
            self.check_updates_on_start
        ));
        s.push_str(&format!(
            "# card_slot1/card_slot2: LEGADO (sem efeito) — os slots nascem vazios\n# em todo jogo; inserir card é manual pelo modal de cards.\ncard_slot1 = \"{}\"\ncard_slot2 = \"{}\"\n\n",
            self.card_slot1.as_deref().unwrap_or(""),
            self.card_slot2.as_deref().unwrap_or("")
        ));
        s.push_str(&format!(
            "# mute_game: silencia o áudio do jogo (fica só o som ambiente do leitor).\nmute_game = {}\n\n",
            self.mute_game
        ));
        s.push_str(&format!(
            "# cheater_fish: ao ligar o console, a tela de cheats (estilo GameShark) antes do jogo.\ncheater_fish = {}\n\n",
            self.cheater_fish
        ));
        s.push_str(&format!(
            "# TV: volume 0..16 e imagem [brilho, contraste, cor, nitidez] 0..100 (50 = neutro).\ntv_volume = {}\ntv_picture = {:?}\n\n",
            self.tv_volume, self.tv_picture
        ));
        if let Some(b) = &self.bios_default {
            s.push_str(&format!(
                "# bios_default: a BIOS ativa (os arquivos ficam em bios/).\nbios_default = \"{}\"\n\n",
                b
            ));
        }
        s.push_str("# RetroAchievements\n");
        s.push_str(&format!("ra_user = {:?}\n", self.ra_user));
        s.push_str(&format!("ra_token = {:?}\n", self.ra_token));
        s.push_str(&format!("ra_hardcore = {}\n\n", self.ra_hardcore));
        if let Some(connect) = &self.ra_connect {
            s.push_str(&format!("ra_connect = {connect:?}\n\n"));
        }
        s.push_str("[keyboard]\n");
        for (action, key) in self.keymap.describe() {
            s.push_str(&format!("{action} = {key:?}\n"));
        }
        s.push_str("\n[gamepad]\n");
        for (action, button) in self.padmap.describe() {
            s.push_str(&format!("{action} = {button:?}\n"));
        }
        s
    }

    /// Write the live config back to `source`. Errors if this `Config` was
    /// never tied to a path.
    pub fn save(&self) -> Result<()> {
        let path = self
            .source
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no config path to save to"))?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        write_config_private(path, &self.to_toml())
            .with_context(|| format!("writing {}", path.display()))
    }
}

/// Grava o config com permissão 0600 no Unix — ele carrega os tokens do
/// RetroAchievements — e de forma atômica (tmp + rename): um travamento no
/// meio não deixa o cfg pela metade.
/// O cfg guarda o token do RA: legível por outros usuários (0644 de um
/// editor de texto, ou de uma migração por rename, que preserva o modo),
/// volta para 0600 já na leitura — não só na próxima gravação.
fn tighten_permissions(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.permissions().mode() & 0o077 != 0 {
                let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
            }
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// O TOML do cfg, com um erro que diz só ONDE quebrou (linha/coluna e a
/// mensagem do parser) — o `Display` do `toml` cita o trecho do arquivo, e
/// a linha quebrada pode ser a do `ra_token`, que iria parar no log.
fn parse_raw(text: &str, path: &std::path::Path) -> Result<Raw> {
    toml::from_str::<Raw>(text).map_err(|e| {
        let (line, col) = e
            .span()
            .map(|span| {
                let before = &text[..span.start.min(text.len())];
                let line = before.matches('\n').count() + 1;
                let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
                (line, col)
            })
            .unwrap_or((0, 0));
        // Erro de tipo/valor do serde cita o VALOR ("invalid type: integer
        // `123…`") — que pode ser um token sem aspas. Só o tipo do erro.
        let msg = e.message();
        let msg = if msg.starts_with("invalid type") || msg.starts_with("invalid value") {
            "valor de tipo inválido"
        } else {
            msg
        };
        anyhow::anyhow!(
            "parsing {}: linha {line}, coluna {col}: {msg}",
            path.display()
        )
    })
}

pub(crate) fn write_config_private(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    polystnx_domain::fsutil::write_atomic_private(path, contents)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> Config {
        Config {
            fullscreen: false,
            check_updates_on_start: true,
            mute_game: false,
            cheater_fish: true,
            tv_volume: 16,
            tv_picture: [50; 4],
            card_slot1: None,
            card_slot2: None,
            bios_default: None,
            ra_user: String::new(),
            ra_token: String::new(),
            ra_hardcore: true,
            ra_connect: None,
            keymap: KeyMap::defaults(),
            padmap: PadMap::defaults(),
            source: None,
        }
    }

    #[test]
    fn cheater_fish_defaults_on_and_round_trips() {
        let mut cfg = defaults();
        assert!(cfg.cheater_fish);
        cfg.apply(toml::from_str("cheater_fish = false").unwrap())
            .unwrap();
        assert!(!cfg.cheater_fish);
        let raw: Raw = toml::from_str(&cfg.to_toml()).unwrap();
        let mut round = defaults();
        round.apply(raw).unwrap();
        assert!(!round.cheater_fish);
    }

    #[test]
    fn tv_settings_round_trip_and_clamp() {
        let mut cfg = defaults();
        assert_eq!((cfg.tv_volume, cfg.tv_picture), (16, [50; 4]));
        cfg.apply(toml::from_str("tv_volume = 99\ntv_picture = [10, 200, 50, 70]").unwrap())
            .unwrap();
        assert_eq!(cfg.tv_volume, 16, "volume tem teto");
        assert_eq!(cfg.tv_picture, [10, 100, 50, 70], "imagem tem teto");
        let raw: Raw = toml::from_str(&cfg.to_toml()).unwrap();
        let mut round = defaults();
        round.apply(raw).unwrap();
        assert_eq!(round.tv_picture, cfg.tv_picture);
    }

    #[test]
    fn empty_config_keeps_defaults() {
        let mut cfg = defaults();
        cfg.apply(toml::from_str("").unwrap()).unwrap();
        assert!(!cfg.fullscreen);
        assert!(cfg.check_updates_on_start);
    }

    #[test]
    fn check_updates_on_start_can_be_turned_off() {
        let mut cfg = defaults();
        cfg.apply(toml::from_str("check_updates_on_start = false").unwrap())
            .unwrap();
        assert!(!cfg.check_updates_on_start);
    }

    #[test]
    fn overrides_and_rebinds_apply() {
        let mut cfg = defaults();
        // "pause" is a pre-revision console-command binding — ignored now,
        // not an error (see `unknown_action_is_ignored`).
        let raw: Raw =
            toml::from_str("fullscreen = true\n[keyboard]\nb = \"Space\"\npause = \"Escape\"")
                .unwrap();
        cfg.apply(raw).unwrap();
        assert!(cfg.fullscreen);
        // Rebound key resolves; describe() reflects it.
        let d = cfg.keymap.describe();
        assert!(d.contains(&("b".to_string(), "Space".to_string())));
    }

    #[test]
    fn unknown_action_is_ignored() {
        // Not an error: a typo, or (more likely) a pre-revision config with a
        // now-removed console-command binding (eject, pause, ...) — either
        // way there's nothing to bind it to, so `apply` warns and moves on
        // rather than refusing to start over one stale line.
        let mut cfg = defaults();
        let raw: Raw = toml::from_str("[keyboard]\nwarp = \"W\"\nb = \"Space\"").unwrap();
        cfg.apply(raw).unwrap();
        assert!(cfg
            .keymap
            .describe()
            .contains(&("b".to_string(), "Space".to_string())));
    }

    #[test]
    fn ra_fields_round_trip_and_default_off() {
        let mut cfg = defaults();
        assert!(cfg.ra_user.is_empty() && cfg.ra_token.is_empty());
        assert!(cfg.ra_hardcore);
        cfg.ra_user = "player1".into();
        cfg.ra_token = "abc123".into();
        cfg.ra_hardcore = false;
        let raw: Raw = toml::from_str(&cfg.to_toml()).unwrap();
        let mut round = defaults();
        round.apply(raw).unwrap();
        assert_eq!(round.ra_user, "player1");
        assert_eq!(round.ra_token, "abc123");
        assert!(!round.ra_hardcore);
    }

    #[test]
    fn to_toml_round_trips() {
        let mut cfg = defaults();
        cfg.fullscreen = true;
        cfg.check_updates_on_start = false;
        let text = cfg.to_toml();
        let raw: Raw = toml::from_str(&text).unwrap();
        let mut round = defaults();
        round.apply(raw).unwrap();
        assert!(round.fullscreen);
        assert!(!round.check_updates_on_start);
        assert_eq!(round.keymap.describe(), cfg.keymap.describe());
        assert_eq!(round.padmap.describe(), cfg.padmap.describe());
    }

    #[test]
    fn gamepad_rebind_applies_and_round_trips() {
        let mut cfg = defaults();
        let raw: Raw = toml::from_str("[gamepad]\nb = \"dpup\"").unwrap();
        cfg.apply(raw).unwrap();
        let d = cfg.padmap.describe();
        // B now lives on dpup; Up lost its button to it (one action per
        // button, one button per action — same rule as the keyboard side).
        assert!(d.contains(&("b".to_string(), "dpup".to_string())));
        assert!(d.contains(&("up".to_string(), String::new())));
        // And the text form survives a save/load round trip.
        let raw: Raw = toml::from_str(&cfg.to_toml()).unwrap();
        let mut round = defaults();
        round.apply(raw).unwrap();
        assert_eq!(round.padmap.describe(), cfg.padmap.describe());
    }

    #[test]
    fn unknown_gamepad_button_is_skipped_not_fatal() {
        let mut cfg = defaults();
        let before = cfg.padmap.describe();
        let raw: Raw = toml::from_str("[gamepad]\nb = \"not_a_button\"").unwrap();
        assert!(cfg.apply(raw).is_ok(), "uma tecla ruim não derruba o cfg");
        assert_eq!(cfg.padmap.describe(), before, "e o mapa fica como estava");
    }

    #[test]
    fn toml_errors_do_not_quote_the_file() {
        let text = "ra_user = \"x\"\nra_token = \"segredo123\" lixo\n";
        let e = parse_raw(text, std::path::Path::new("polystnx.cfg"))
            .err()
            .expect("tinha de falhar");
        let msg = format!("{e:#}");
        assert!(!msg.contains("segredo123"), "{msg}");
        assert!(msg.contains("linha 2"), "{msg}");
        // Token sem aspas, só números: erro de tipo — o valor não aparece.
        let e = parse_raw(
            "ra_token = 98765432\n",
            std::path::Path::new("polystnx.cfg"),
        )
        .err()
        .expect("tipo errado");
        assert!(!format!("{e:#}").contains("98765432"), "{e:#}");
    }

    #[test]
    fn invalid_utf8_default_cfg_moves_aside() {
        let dir = std::env::temp_dir().join(format!("polystnx-cfg-utf8-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("polystnx.cfg");
        std::fs::write(&path, [0xff, 0xfe, b'x']).unwrap();
        assert!(Config::load_at(Some(path.clone()), false).is_ok());
        assert!(dir.join("polystnx.cfg.bad").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn broken_default_cfg_moves_aside_and_loads_defaults() {
        let dir = std::env::temp_dir().join(format!("polystnx-cfg-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("polystnx.cfg");
        std::fs::write(&path, "isto [não é toml").unwrap();

        let cfg = Config::load_at(Some(path.clone()), false).unwrap();
        assert!(cfg.fullscreen, "padrões de volta");
        let bad = dir.join("polystnx.cfg.bad");
        assert_eq!(std::fs::read_to_string(&bad).unwrap(), "isto [não é toml");
        assert!(path.exists(), "um cfg padrão novo foi gravado");

        // Explícito: o erro continua sendo erro, e nada sai do lugar.
        std::fs::write(&path, "isto [não é toml").unwrap();
        assert!(Config::load_at(Some(path.clone()), true).is_err());
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn readable_cfg_is_tightened_on_load() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("polystnx-cfg-perm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("polystnx.cfg");
        std::fs::write(&path, "fullscreen = false\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        Config::load_at(Some(path.clone()), false).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
