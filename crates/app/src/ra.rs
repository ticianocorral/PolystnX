//! RetroAchievements client (plan: `docs/plano-retroachievements.md`,
//! fase 1) — account only for now: the login test against the real API.
//! Later phases grow this into game identification and the unlock runtime;
//! everything here is inert unless the user configured a token, and every
//! call is opt-in (the only automatic one would be a later "identify on
//! cart insert", still gated on a configured account).
//!
//! Auth is the classic web-API form (username `u` + web key `y`) — the key
//! the user generates on retroachievements.org under *Settings → Web API*.
//! Third-party clients are welcome as long as they identify themselves, so
//! the User-Agent names the app and its version.

use std::io::Read;
use std::time::Duration;

/// Mensagens só em português (o par en/pt do SixteeN fica na fonte para o
/// port seguir 1:1).
macro_rules! tr {
    ($en:expr, $pt:expr $(,)?) => {
        $pt
    };
}

pub const API_BASE: &str = "https://retroachievements.org";

/// Identifies the app to the RA server in every call (third-party clients
/// are welcome as long as they name themselves).
const USER_AGENT: &str = concat!("polystnx/", env!("CARGO_PKG_VERSION"));

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()
}

/// Teto do corpo JSON de uma resposta — o `into_json` do ureq 2 lê sem
/// limite, e um servidor com bug (ou hostil) não pode encher a memória.
/// As respostas reais ficam em dezenas de KB; 8 MiB sobra.
const MAX_JSON_BYTES: u64 = 8 << 20;

/// `into_json` com teto: desserializa no máximo [`MAX_JSON_BYTES`] do corpo
/// (um corpo maior chega truncado e falha como JSON inválido).
fn read_json<T: serde::de::DeserializeOwned>(resp: ureq::Response) -> serde_json::Result<T> {
    serde_json::from_reader(resp.into_reader().take(MAX_JSON_BYTES))
}

/// Percent-encode the few characters a username/token can carry that would
/// break a query string — full urlencoding machinery isn't a dependency
/// worth adding for two fields.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Validate the configured credentials against the real API — returns a
/// human label for the settings row (`"<user> — <N> pontos"`), or an error
/// message that fits in the same row. Runs on the caller's thread; the
/// settings screen wraps it in the standard worker + `mpsc` shape.
pub fn test_login(user: &str, token: &str) -> Result<String, String> {
    if user.is_empty() || token.is_empty() {
        return Err(tr!("fill in the username and token", "preencha usuário e token").to_string());
    }
    let resp = agent()
        .get(&format!(
            "{API_BASE}/API/API_GetUserProfile.php?u={}&y={}",
            encode(user),
            encode(token)
        ))
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(net_err)?;
    let body: serde_json::Value =
        read_json(resp).map_err(|e| tr!(format!("response: {e}"), format!("resposta: {e}")))?;
    // Failure shapes: {"Success": false} or an error code object; success
    // carries the profile with at least "User".
    if body.get("Success").is_some_and(|s| s == false) {
        return Err(tr!("invalid username or token", "usuário ou token inválidos").to_string());
    }
    let Some(name) = body.get("User").and_then(|v| v.as_str()) else {
        return Err(tr!("unexpected response", "resposta inesperada").to_string());
    };
    let points = body.get("TotalPoints").and_then(|v| v.as_i64());
    Ok(match points {
        Some(p) => tr!(
            format!("{name} — {p} points"),
            format!("{name} — {p} pontos")
        ),
        None => name.to_string(),
    })
}

// ---- fase 2: game identification ------------------------------------

/// What phase 2 needs to know about a game: its RA title and how many
/// achievements its set has. The unlock runtime (phase 3) will grow this
/// with the conditions themselves.
///
/// `unsupported`: o RA marcou este **hash** como versão não suportada e
/// respondeu com o set fictício de aviso (0 conquistas reais) — o botão
/// Conquistas aparece desativado com o motivo (plan revision).
#[derive(Debug, Clone)]
pub struct RaGame {
    pub title: String,
    pub achievements: usize,
    pub unsupported: bool,
}

/// Where the per-game cache lives — `retroachievements/ra-cache/<hash>.json`, one file
/// per identified game, holding the raw `API_GetGameExtended` reply so a
/// later phase can pull conditions/badges from it without re-fetching.
fn cache_path(hash: &str) -> std::path::PathBuf {
    crate::dirs::retroachievements_dir()
        .join("ra-cache")
        .join(format!("{hash}.json"))
}

/// The RA hash of the disc at `path` — o `rhash` de disco do rcheevos
/// (algoritmo validado contra o site na Fase 0); chamado uma vez por jogo
/// por visita da estante.
pub fn hash_rom(path: &std::path::Path) -> Result<String, String> {
    polystnx_ra::hash::psx_disc_hash(path)
}

/// The cached identification for `hash`, if a previous fetch left one.
/// A legacy cache (from the old Extended route — hashed `MemAddr`, no
/// `GameID`/`Sets`) reads as absent so the shelf refetches it through the
/// Connect route.
pub fn cached_game(hash: &str) -> Option<RaGame> {
    let text = std::fs::read_to_string(cache_path(hash)).ok()?;
    if text.trim().is_empty() {
        return None; // known-unknown
    }
    let body: serde_json::Value = serde_json::from_str(&text).ok()?;
    if body.get("GameID").is_none() && body.get("Sets").is_none() {
        return None; // legacy shape — stale, refetch
    }
    game_from_json(&body)
}

/// Whether a previous fetch concluded "the server doesn't know this hash"
/// — an empty cache file, so the shelf never re-asks.
pub fn is_cached_unknown(hash: &str) -> bool {
    std::fs::metadata(cache_path(hash))
        .map(|m| m.len() == 0)
        .unwrap_or(false)
}

/// Parse the cached JSON into the phase-2 summary. Handles both shapes the
/// Connect `achievementsets` route serves: the flat legacy patch
/// (`Title` + `Achievements` map) and the multiset one (`Sets` array).
/// `None` when the shape isn't what we expect — treated as not-identified
/// rather than an error.
fn game_from_json(body: &serde_json::Value) -> Option<RaGame> {
    let title = body
        .get("Title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            body.get("Sets")
                .and_then(|s| s.as_array())
                .and_then(|s| s.first())
                .and_then(|s| s.get("Title"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })?;
    let all = achievements_json(body);
    let achievements = all
        .iter()
        .filter(|a| {
            a.get("ID").and_then(|v| v.as_u64()).map(|v| v as u32) != Some(CLIENT_WARNING_ID)
        })
        .count();
    // Set de aviso — o RA marca hashes incompatíveis assim (caso Batman
    // Forever Rev 1). IMPORTANTE: o achievement-aviso também é injetado em
    // respostas NORMAIS para clientes fora do registro oficial, então só é
    // "não suportado" quando NÃO sobra conquista real nenhuma.
    let unsupported = achievements == 0
        && (all.iter().any(|a| {
            a.get("ID").and_then(|v| v.as_u64()).map(|v| v as u32) == Some(CLIENT_WARNING_ID)
        }) || title.starts_with("Unsupported Game Version"));
    Some(RaGame {
        title,
        achievements,
        unsupported,
    })
}

/// The achievement objects of either response shape: the flat patch keeps
/// them in a top-level map, the multiset one nests arrays under `Sets`.
fn achievements_json(body: &serde_json::Value) -> Vec<&serde_json::Value> {
    if let Some(map) = body.get("Achievements").and_then(|a| a.as_object()) {
        return map.values().collect();
    }
    body.get("Sets")
        .and_then(|s| s.as_array())
        .map(|sets| {
            sets.iter()
                .filter_map(|s| s.get("Achievements").and_then(|a| a.as_array()))
                .flatten()
                .collect()
        })
        .unwrap_or_default()
}

/// Ask the server which game `hash` is and arm the runtime: the Connect
/// route `r=achievementsets` resolves the hash straight to the set AND
/// returns the real `MemAddr` logic (plan revision — the old
/// `API_GetGameExtended` now serves a hashed MemAddr to every client, so
/// the definitions only come from here, authenticated with the login
/// token from `connect_login`). Cached verbatim next to the saves.
/// Ok(None) = the server doesn't know this hash (cached as an empty file
/// so it's never asked twice). Meant for a worker thread; the shelf
/// calls it throttled to one in flight.
pub fn fetch_game(
    user: &str,
    web_key: &str,
    connect: &str,
    hash: &str,
) -> Result<Option<RaGame>, String> {
    if connect.is_empty() {
        return Err(tr!(
            "link the account (password) in settings > achievements",
            "conecte a conta (senha) em configurações > conquistas"
        )
        .to_string());
    }
    let resp = agent()
        .post(&format!("{API_BASE}/dorequest.php"))
        .set("User-Agent", USER_AGENT)
        .send_form(&[
            ("r", "achievementsets"),
            ("u", user),
            ("t", connect),
            ("m", hash),
        ])
        .map_err(net_err)?;
    let body: serde_json::Value = read_json(resp).map_err(|e| format!("resposta: {e}"))?;
    if body.get("Success").is_some_and(|s| s == false) {
        let err = body
            .get("Error")
            .and_then(|v| v.as_str())
            .unwrap_or("recusado");
        return Err(err.to_string());
    }
    let Some(game) = game_from_json(&body) else {
        // Server answered but the shape holds no game: treat as
        // not-registered (cached empty) rather than retrying forever.
        let _ = write_cache(hash, "");
        return Ok(None);
    };
    let text = serde_json::to_string(&body).unwrap_or_default();
    write_cache(hash, &text)?;

    // A rota Connect não traz lançamento/editora (campos que o painel
    // mostra): completa com a rota de metadados (web key) quando existe —
    // gravados por cima do cache, `release_and_extras` lê dos dois shapes.
    if !web_key.is_empty() {
        let game_id = cached_game_id(hash);
        if let Some(game_id) = game_id {
            if let Ok(meta) = agent()
                .get(&format!(
                    "{API_BASE}/API/API_GetGameExtended.php?i={game_id}&u={}&y={}",
                    encode(user),
                    encode(web_key)
                ))
                .set("User-Agent", USER_AGENT)
                .call()
            {
                if let Ok(meta) = read_json::<serde_json::Value>(meta) {
                    let mut cached: serde_json::Value =
                        serde_json::from_str(&text).unwrap_or_else(|_| body.clone());
                    // Indexar um Value que não é objeto (array/null de um
                    // servidor com bug) dá panic — só mescla em objeto.
                    if !cached.is_object() {
                        cached = serde_json::json!({});
                    }
                    cached["Released"] = meta.get("Released").cloned().unwrap_or_default();
                    cached["Publisher"] = meta.get("Publisher").cloned().unwrap_or_default();
                    cached["Developer"] = meta.get("Developer").cloned().unwrap_or_default();
                    cached["Genre"] = meta.get("Genre").cloned().unwrap_or_default();
                    write_cache(hash, &serde_json::to_string(&cached).unwrap_or_default())?;
                }
            }
        }
    }

    Ok(Some(game))
}

fn write_cache(hash: &str, text: &str) -> Result<(), String> {
    let path = cache_path(hash);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("cache: {e}"))?;
    }
    polystnx_domain::fsutil::write_atomic(&path, text).map_err(|e| format!("cache: {e}"))
}

// ---- fase 3: live session -------------------------------------------

use polystnx_ra::runtime::{Achievement, Session};

/// O RA injeta um "achievement" de aviso em toda resposta para clientes
/// que não estão no registro oficial de emuladores (`CLIENT_WARNING_ID` no
/// RAWeb; pop em 5 s, `MemAddr` "1=1.300."). Não é conquista: filtrado
/// antes de ativar, com o texto do aviso logado.
const CLIENT_WARNING_ID: u32 = 101_000_001;

/// Parse the cached Connect JSON into the runtime's achievement list (id,
/// texts, points, badge, `MemAddr` definition). Both response shapes (flat
/// and multiset); achievements whose `MemAddr` is a bare 32-hex digest have
/// no logic behind them (legacy redacted cache) and are skipped — a cache
/// like that is stale and gets refetched anyway.
pub fn parse_achievements(body: &serde_json::Value) -> Vec<Achievement> {
    let mut out = Vec::new();
    for a in achievements_json(body) {
        let (Some(id), Some(title), Some(memaddr)) = (
            a.get("ID").and_then(|v| v.as_u64()).map(|v| v as u32),
            a.get("Title").and_then(|v| v.as_str()),
            a.get("MemAddr").and_then(|v| v.as_str()),
        ) else {
            continue;
        };
        if id == CLIENT_WARNING_ID {
            log::warn!(
                "ra aviso do servidor: {} — {}",
                title,
                a.get("Description").and_then(|v| v.as_str()).unwrap_or("")
            );
            continue;
        }
        // A real definition starts with an address ("0xH0042=…", "d0xH…=…");
        // a bare 32-hex string is the redacted hash of one (pre-Connect
        // cache) — nothing to activate.
        let has_logic = !(memaddr.len() == 32
            && !memaddr.starts_with("0x")
            && memaddr.bytes().all(|b| b.is_ascii_hexdigit()));
        if !has_logic {
            continue;
        }
        out.push(Achievement {
            id,
            title: title.to_string(),
            description: a
                .get("Description")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            points: points_from_json(a),
            badge: a
                .get("BadgeName")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            memaddr: memaddr.to_string(),
        });
    }
    out
}

/// O `Points` do cache chega como número JSON (`5`) — e como string na
/// resposta do endpoint de progresso. Aceitar os dois; 0 quando não vier.
fn points_from_json(a: &serde_json::Value) -> u32 {
    match a.get("Points") {
        Some(serde_json::Value::Number(n)) => n.as_u64().unwrap_or(0) as u32,
        Some(serde_json::Value::String(s)) => s.parse().unwrap_or(0),
        _ => 0,
    }
}

/// Assinatura de validação que o `awardachievement` espera (`v`): MD5 de
/// id da conquista + usuário + flag hardcore ("0"/"1"), concatenados — o
/// mesmo que o rcheevos manda em `rc_api_init_award_achievement_request`.
fn award_signature(achievement_id: u32, user: &str, hardcore: bool) -> String {
    use md5::Digest;
    let mut h = md5::Md5::new();
    h.update(format!("{achievement_id}{user}{}", u8::from(hardcore)));
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Award one unlocked achievement through the Connect route —
/// authenticated with the login token (`connect_login`), not the web key.
/// `hardcore` maps straight to the API's `h` flag. Returns the server
/// message on success.
pub fn award(
    user: &str,
    token: &str,
    achievement_id: u32,
    hash: &str,
    hardcore: bool,
) -> Result<String, String> {
    let resp = agent()
        .post(&format!("{API_BASE}/dorequest.php"))
        .set("User-Agent", USER_AGENT)
        .send_form(&[
            ("r", "awardachievement"),
            ("u", user),
            ("t", token),
            ("a", &achievement_id.to_string()),
            ("h", if hardcore { "1" } else { "0" }),
            ("m", hash),
            ("v", &award_signature(achievement_id, user, hardcore)),
        ])
        // Qualquer status de erro (401 de token revogado, 429, 5xx) leva o
        // prefixo `HTTP:` — falha de transporte/credencial, nunca "recusa":
        // só um 200 com `Success:false` recusa o envio em si.
        .map_err(|e| format!("HTTP: {}", net_err(e)))?;
    let body: serde_json::Value = read_json(resp).map_err(|e| format!("resposta: {e}"))?;
    if body.get("Success").is_some_and(|s| s == false) {
        let err = body
            .get("Error")
            .and_then(|v| v.as_str())
            .unwrap_or("recusado");
        return Err(err.to_string());
    }
    Ok(body
        .get("Score")
        .map(|s| s.to_string())
        .unwrap_or_else(|| "ok".into()))
}

/// A request failure as a message for the settings row / log — WITHOUT
/// the URL: ureq's own `Display` prints the full request URL, and the web
/// API routes carry the key in the query string. An HTTP error status
/// keeps the server's `Error` text when the body has one (a wrong
/// password comes back as 401 with JSON).
fn net_err(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, resp) => read_json::<serde_json::Value>(resp)
            .ok()
            .and_then(|b| b.get("Error").and_then(|v| v.as_str()).map(str::to_string))
            .unwrap_or_else(|| format!("HTTP {code}")),
        ureq::Error::Transport(t) => {
            tr!(
                format!("network: {}", t.kind()),
                format!("rede: {}", t.kind())
            )
        }
    }
}

/// Exchange the account password for a long-lived Connect login token —
/// what the logic (`achievementsets`) and submission (`awardachievement`)
/// routes authenticate with. The password itself is never stored.
pub fn connect_login(user: &str, password: &str) -> Result<String, String> {
    let resp = agent()
        .post(&format!("{API_BASE}/dorequest.php"))
        .set("User-Agent", USER_AGENT)
        .send_form(&[("r", "login2"), ("u", user), ("p", password)])
        .map_err(net_err)?;
    let body: serde_json::Value =
        read_json(resp).map_err(|e| tr!(format!("response: {e}"), format!("resposta: {e}")))?;
    if body.get("Success").is_some_and(|s| s == false) {
        let err = body
            .get("Error")
            .and_then(|v| v.as_str())
            .unwrap_or(tr!("login refused", "login recusado"));
        return Err(err.to_string());
    }
    body.get("Token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| tr!("no token in response", "resposta sem token").to_string())
}

/// Where a game's "already earned" ids live — `retroachievements/ra-earned/<hash>.json`.
/// The runtime re-arms on reset, so this local set is what keeps unlocks
/// from being submitted twice.
fn earned_path(hash: &str) -> std::path::PathBuf {
    crate::dirs::retroachievements_dir()
        .join("ra-earned")
        .join(format!("{hash}.json"))
}

/// Where a game's serialized rcheevos session lives —
/// `retroachievements/ra-progress/<hash>.rap`.
pub fn progress_path(hash: &str) -> std::path::PathBuf {
    crate::dirs::retroachievements_dir()
        .join("ra-progress")
        .join(format!("{hash}.rap"))
}

/// As conquistas ganhas de um jogo: id → `true` quando ganha em hardcore,
/// `false` em softcore (plan revision: "como sei qual tipo ganhei ou já
/// tenho?"). Persistido em `retroachievements/ra-earned/<hash>.json` como
/// `{"id": hardcore}` — o formato legado (array de ids, sem modo) é lido
/// como tudo softcore e corrigido pelo sync do servidor.
pub type EarnedMap = std::collections::HashMap<u32, bool>;

fn load_earned(hash: &str) -> EarnedMap {
    let Some(text) = std::fs::read_to_string(earned_path(hash)).ok() else {
        return EarnedMap::new();
    };
    if let Ok(map) = serde_json::from_str::<EarnedMap>(&text) {
        return map;
    }
    // Legado: array plano de ids, sem modo.
    serde_json::from_str::<Vec<u32>>(&text)
        .unwrap_or_default()
        .into_iter()
        .map(|id| (id, false))
        .collect()
}

fn save_earned(hash: &str, earned: &EarnedMap) {
    let path = earned_path(hash);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(t) = serde_json::to_string(earned) {
        let _ = polystnx_domain::fsutil::write_atomic(&path, t);
    }
}

/// What one unlocked achievement looks like to the UI — the OSD block and
/// the submit worker both get one of these.
#[derive(Debug, Clone)]
pub struct Unlock {
    pub id: u32,
    pub title: String,
    pub points: u32,
    /// BadgeName — the badge image id on the RA CDN.
    pub badge: String,
    /// `true` quando o desbloqueio vale hardcore (a sessão inteira define
    /// o modo — plan revision: "como sei qual tipo ganhei?").
    pub hardcore: bool,
}

/// A live RA session for the inserted cartridge. `None` everywhere the
/// account isn't configured / the game isn't on RA.
pub struct Active {
    pub hash: String,
    pub game_title: String,
    pub hardcore: bool,
    session: Session,
    earned: EarnedMap,
    pub user: String,
    pub token: String,
}

impl Active {
    /// Identify the ROM at `path` and arm the runtime. `Ok(None)` = the
    /// account is on but this ROM has no achievement set (or isn't on RA);
    /// `Err` = account/cache trouble worth a log line.
    pub fn start(
        path: &std::path::Path,
        user: &str,
        token: &str,
        hardcore: bool,
    ) -> Result<Option<Active>, String> {
        // Sessão logada: o que ficou pendente de uma execução anterior
        // (app fechado no meio do envio, rede fora) vai em segundo plano.
        resubmit_pending(user, token);
        let hash = hash_rom(path)?;
        let Some(cached) = cached_game(&hash) else {
            return Ok(None);
        };
        // The full API reply was cached verbatim (fase 2) — parse the
        // achievements out of it again here.
        let json: serde_json::Value = match std::fs::read_to_string(cache_path(&hash))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
        {
            Some(j) => j,
            None => return Ok(None),
        };
        let achievements = parse_achievements(&json);
        if achievements.is_empty() {
            return Ok(None);
        }
        let mut session = Session::from_parts(achievements);
        let progress = std::fs::read(progress_path(&hash)).unwrap_or_default();
        session.load_progress(&progress);
        let earned = load_earned(&hash);
        // Progress restored: everything TRIGGERED counts as earned even if
        // the earned file was lost.
        Ok(Some(Active {
            hash,
            game_title: cached.title,
            hardcore,
            session,
            earned,
            user: user.to_string(),
            token: token.to_string(),
        }))
    }

    /// Evaluate one frame against the work RAM. Returns unlocks to notify
    /// and submit (already persisted as earned — a crash can't double-award
    /// even if the submit is lost; the server deduplicates anyway).
    pub fn tick(&mut self, ram: &[u8]) -> Vec<Unlock> {
        let mut unlocks = Vec::new();
        for id in self.session.tick(ram) {
            // Já ganho permanece ganho — exceto o upgrade softcore →
            // hardcore: a sessão hardcore re-dispara e o servidor faz o
            // upgrade no re-award.
            match self.earned.get(&id) {
                Some(&true) => continue,
                Some(&false) if !self.hardcore => continue,
                _ => {}
            }
            self.earned.insert(id, self.hardcore);
            save_earned(&self.hash, &self.earned);
            if let Some(a) = self.session.achievements.iter().find(|a| a.id == id) {
                unlocks.push(Unlock {
                    id: a.id,
                    title: a.title.clone(),
                    points: a.points,
                    badge: a.badge.clone(),
                    hardcore: self.hardcore,
                });
            }
        }
        unlocks
    }

    /// Which achievements are earned (runtime state ∪ local set), for the
    /// pause-book listing.
    pub fn earned_snapshot(&self) -> (usize, EarnedMap) {
        let total = self.session.achievements.len();
        let mut earned = self.earned.clone();
        let mut mudou = false;
        for id in self.session.triggered_ids() {
            if let std::collections::hash_map::Entry::Vacant(e) = earned.entry(id) {
                e.insert(self.hardcore);
                mudou = true;
            }
        }
        if mudou {
            save_earned(&self.hash, &earned);
        }
        (total, earned)
    }

    /// The achievement list, for the pause book.
    pub fn achievements(&self) -> &[Achievement] {
        &self.session.achievements
    }

    /// Bank hit counts / trigger state for the next power-on.
    pub fn save_progress(&mut self) {
        let blob = self.session.save_progress();
        if !blob.is_empty() {
            let path = progress_path(&self.hash);
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = polystnx_domain::fsutil::write_atomic(&path, blob);
        }
    }

    /// Console power-on: hit counts re-arm. Earned stays authoritative.
    pub fn reset(&mut self) {
        self.session.reset();
    }

    /// Reload the earned set from disk (the shelf/session server-sync
    /// merges unlocks earned elsewhere into it) so a synced unlock is
    /// never re-submitted by this session.
    pub fn refresh_earned(&mut self) {
        self.earned = load_earned(&self.hash);
    }

    /// Une os ids que o servidor tem (o worker da busca devolve o union
    /// disco ∪ servidor) no set da sessão, em memória e no disco: a modal
    /// in-game marca conquistas ganhas fora deste app sem depender de a
    /// lista da estante já ter sincronizado.
    pub fn absorb_earned(&mut self, ids: &EarnedMap) {
        let antes = (self.earned.len(), self.hardcore_count());
        for (&id, &hardcore) in ids {
            match self.earned.get(&id) {
                Some(true) => {}
                Some(false) if hardcore => {
                    self.earned.insert(id, true);
                }
                None => {
                    self.earned.insert(id, hardcore);
                }
                _ => {}
            }
        }
        if (self.earned.len(), self.hardcore_count()) != antes {
            save_earned(&self.hash, &self.earned);
        }
    }

    fn hardcore_count(&self) -> usize {
        self.earned.values().filter(|&&hardcore| hardcore).count()
    }

    /// Envio de um desbloqueio (worker thread, 3 tentativas). O registro vai
    /// para `ra-pending.jsonl` ANTES de qualquer rede e só sai de lá quando o
    /// servidor aceita (ou diz que já tinha): se o app fechar no meio, ou as
    /// tentativas acabarem, o próximo login reenvia ([`resubmit_pending`]).
    pub fn submit_unlock(&self, unlock: &Unlock) {
        let entry = PendingAward {
            user: self.user.clone(),
            hash: self.hash.clone(),
            id: unlock.id,
            hardcore: self.hardcore,
            title: unlock.title.clone(),
            refusals: 0,
        };
        let token = self.token.clone();
        // A pendência vai para o disco AQUI, antes da thread: se o app
        // fechar logo depois do desbloqueio, a thread pode nem rodar — o
        // registro já está salvo para o reenvio do próximo login. É uma
        // escrita pequena e rara (um desbloqueio).
        let path = pending_path();
        if let Err(e) = pending_add(&path, &entry) {
            log::warn!("ra: pendente {} não gravado ({e})", entry.id);
        }
        std::thread::spawn(move || {
            for attempt in 0..3 {
                match award(&entry.user, &token, entry.id, &entry.hash, entry.hardcore) {
                    Ok(msg) => {
                        log::info!(
                            "ra: conquista {} enviada ({msg}, tentativa {})",
                            entry.id,
                            attempt + 1
                        );
                        let _ = pending_remove(&path, &entry);
                        return;
                    }
                    Err(e) if is_already_awarded(&e) => {
                        log::info!("ra: conquista {} já estava no servidor ({e})", entry.id);
                        let _ = pending_remove(&path, &entry);
                        return;
                    }
                    Err(e) => {
                        log::warn!("ra: submit {} falhou ({e})", entry.id);
                        std::thread::sleep(Duration::from_secs(2));
                    }
                }
            }
            // Sem mais tentativas: o registro fica no pendente para o
            // reenvio do próximo login.
            log::warn!("ra: conquista {} fica pendente", entry.id);
        });
    }
}

// ---- envios pendentes -------------------------------------------------

/// Um desbloqueio ainda não confirmado pelo servidor — uma linha JSON de
/// `retroachievements/ra-pending.jsonl`, com tudo o que o `award` precisa
/// (o token não: vem da sessão logada no reenvio).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct PendingAward {
    /// Vazio nas linhas do formato legado (TSV, sem usuário): reenviadas
    /// com a conta logada.
    #[serde(default)]
    user: String,
    hash: String,
    id: u32,
    hardcore: bool,
    /// Só para o log / leitura humana do arquivo.
    #[serde(default)]
    title: String,
    /// Quantas vezes o servidor RECUSOU este envio (recusa de verdade, não
    /// falha de rede): depois de [`MAX_REFUSALS`] ele sai da fila, em vez de
    /// ser reenviado a cada cartucho inserido para sempre.
    #[serde(default)]
    refusals: u32,
}

/// Recusas do servidor até um pendente ser descartado.
const MAX_REFUSALS: u32 = 3;

/// Falha que não diz nada sobre o envio (sem rede, servidor fora, resposta
/// ilegível) — o pendente fica para a próxima, sem contar recusa.
fn is_transient_failure(err: &str) -> bool {
    err.starts_with("HTTP")
        || err.starts_with("rede:")
        || err.starts_with("network:")
        || err.starts_with("resposta:")
        || err.starts_with("HTTP 5")
        || err.starts_with("HTTP 429")
}

/// Conta uma recusa do servidor; na [`MAX_REFUSALS`]ª, tira da fila.
fn pending_refused(path: &std::path::Path, entry: &PendingAward) -> std::io::Result<()> {
    mutate_pending(path, |all| {
        for e in all.iter_mut().filter(|e| e.same_award(entry)) {
            e.refusals += 1;
        }
        all.retain(|e| {
            let keep = e.refusals < MAX_REFUSALS;
            if !keep {
                log::warn!(
                    "ra: conquista {} recusada {} vezes — descartada",
                    e.id,
                    e.refusals
                );
            }
            keep
        });
    })
}

impl PendingAward {
    /// Mesmo envio = mesmo usuário, jogo, conquista e modo (o título não
    /// conta).
    fn same_award(&self, other: &PendingAward) -> bool {
        self.user == other.user
            && self.hash == other.hash
            && self.id == other.id
            && self.hardcore == other.hardcore
    }
}

fn pending_path() -> std::path::PathBuf {
    crate::dirs::retroachievements_dir().join("ra-pending.jsonl")
}

/// Serializa toda mutação do arquivo de pendentes: o worker de cada
/// desbloqueio e o reenvio do login fazem read-modify-write no mesmo
/// arquivo, e sem a trava um sobrescreveria o que o outro acabou de pôr.
static PENDING_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Uma linha do arquivo: JSON (formato atual) ou o TSV legado
/// `hash\tid\thardcore\ttítulo`. Linha inválida = `None` (ignorada).
fn parse_pending_line(line: &str) -> Option<PendingAward> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    if line.starts_with('{') {
        return serde_json::from_str(line).ok();
    }
    let mut parts = line.splitn(4, '\t');
    let hash = parts.next()?.to_string();
    let id = parts.next()?.parse().ok()?;
    let hardcore = match parts.next()? {
        "1" => true,
        "0" => false,
        _ => return None,
    };
    let title = parts.next().unwrap_or_default().to_string();
    (!hash.is_empty()).then_some(PendingAward {
        user: String::new(),
        hash,
        id,
        hardcore,
        title,
        refusals: 0,
    })
}

/// Os pendentes do arquivo, na ordem; linhas inválidas são puladas.
fn read_pending(path: &std::path::Path) -> Vec<PendingAward> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(parse_pending_line)
        .collect()
}

/// O único caminho que muda o arquivo: trava, lê, aplica `f`, regrava
/// atômico (só quando algo mudou). Linhas inválidas somem na regravação.
fn mutate_pending(
    path: &std::path::Path,
    f: impl FnOnce(&mut Vec<PendingAward>),
) -> std::io::Result<()> {
    let _guard = PENDING_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let before = read_pending(path);
    let mut entries = before.clone();
    f(&mut entries);
    if entries == before {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut text = String::new();
    for e in &entries {
        if let Ok(line) = serde_json::to_string(e) {
            text.push_str(&line);
            text.push('\n');
        }
    }
    polystnx_domain::fsutil::write_atomic(path, text)
}

/// Põe um envio no pendente (sem duplicar o mesmo envio).
fn pending_add(path: &std::path::Path, entry: &PendingAward) -> std::io::Result<()> {
    mutate_pending(path, |all| {
        if !all.iter().any(|e| e.same_award(entry)) {
            all.push(entry.clone());
        }
    })
}

/// Tira um envio confirmado do pendente (todas as cópias dele).
fn pending_remove(path: &std::path::Path, entry: &PendingAward) -> std::io::Result<()> {
    mutate_pending(path, |all| all.retain(|e| !e.same_award(entry)))
}

/// O servidor recusou porque o usuário já tem a conquista ("User already
/// has this achievement unlocked…") — para o pendente vale como sucesso.
fn is_already_awarded(err: &str) -> bool {
    err.to_ascii_lowercase().contains("already")
}

/// Um reenvio por vez no processo (cada cartucho inserido chama).
static RESUBMIT_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Reenvia em segundo plano os pendentes do usuário logado (e os legados
/// sem usuário), tirando do arquivo os que o servidor aceitar ou já tiver.
/// Não bloqueia: worker thread, mesmo agent/timeouts do `award`.
pub fn resubmit_pending(user: &str, token: &str) {
    use std::sync::atomic::Ordering;
    if user.is_empty() || token.is_empty() {
        return;
    }
    if RESUBMIT_RUNNING.swap(true, Ordering::AcqRel) {
        return;
    }
    let (user, token) = (user.to_string(), token.to_string());
    std::thread::spawn(move || {
        let path = pending_path();
        let todo: Vec<PendingAward> = read_pending(&path)
            .into_iter()
            .filter(|e| e.user.is_empty() || e.user == user)
            .collect();
        let mut sent = 0usize;
        for entry in &todo {
            match award(&user, &token, entry.id, &entry.hash, entry.hardcore) {
                Ok(_) => {}
                Err(e) if is_already_awarded(&e) => {}
                Err(e) => {
                    log::info!("ra: pendente {} segue pendente ({e})", entry.id);
                    if !is_transient_failure(&e) {
                        let _ = pending_refused(&path, entry);
                    }
                    continue;
                }
            }
            let _ = pending_remove(&path, entry);
            sent += 1;
        }
        if sent > 0 {
            log::info!("ra: {sent} conquista(s) pendente(s) reenviada(s)");
        }
        RESUBMIT_RUNNING.store(false, Ordering::Release);
    });
}

// ---- shelf view (fase 2/4) -------------------------------------------

/// Everything the shelf's achievements view needs for one game, straight
/// from the local caches — `None` when the game isn't identified (no cache
/// from fase 2's identification).
pub struct ShelfAchievements {
    pub title: String,
    pub achievements: Vec<Achievement>,
    pub earned: EarnedMap,
}

impl ShelfAchievements {
    /// Total points across the set / points already earned.
    pub fn points(&self) -> (u64, u64) {
        let all: u64 = self.achievements.iter().map(|a| a.points as u64).sum();
        let got: u64 = self
            .achievements
            .iter()
            .filter(|a| self.earned.contains_key(&a.id))
            .map(|a| a.points as u64)
            .sum();
        (all, got)
    }
}

/// The shelf panel's achievement tally for the ROM at `path` — (earned,
/// total) straight from the local caches, `None` when the game isn't
/// identified. Lighter than `shelf_achievements` on purpose: two small JSON
/// reads, no achievement-list parsing — the panel rebuilds every frame.
pub fn achievement_tally(rom_path: &std::path::Path) -> Option<(usize, usize)> {
    let hash = hash_rom(rom_path).ok()?;
    let game = cached_game(&hash)?;
    let earned = load_earned(&hash).len();
    Some((earned.min(game.achievements), game.achievements))
}

/// Load the list for the ROM at `path` from the local caches (no network).
pub fn shelf_achievements(rom_path: &std::path::Path) -> Option<ShelfAchievements> {
    let hash = hash_rom(rom_path).ok()?;
    let text = std::fs::read_to_string(cache_path(&hash)).ok()?;
    if text.trim().is_empty() {
        return None;
    }
    let body: serde_json::Value = serde_json::from_str(&text).ok()?;
    let title = body.get("Title")?.as_str()?.to_string();
    let achievements = parse_achievements(&body);
    if achievements.is_empty() {
        return None;
    }
    Some(ShelfAchievements {
        title,
        achievements,
        earned: load_earned(&hash),
    })
}

// ---- progresso do usuário (a fonte do "X de Y (Z%)" do painel) --------

/// The game's numeric RA id from the cached `API_GetGameExtended` reply.
fn cached_game_id(hash: &str) -> Option<u64> {
    let text = std::fs::read_to_string(cache_path(hash)).ok()?;
    let body: serde_json::Value = serde_json::from_str(&text).ok()?;
    // Connect: `GameId` no topo (ou dentro do primeiro set — a resposta
    // multiset não repete no topo); legacy Extended: `ID`.
    body.get("GameID")
        .or_else(|| body.get("GameId"))
        .or_else(|| body.get("ID"))
        .and_then(|v| v.as_u64())
        .or_else(|| {
            body.get("Sets")
                .and_then(|s| s.as_array())
                .and_then(|s| s.first())
                .and_then(|s| s.get("GameId"))
                .and_then(|v| v.as_u64())
        })
}

/// O progresso do usuário num jogo, pelo completion progress: conquistadas,
/// tamanho do set e o prêmio do jogo quando o servidor concedeu um
/// (`HighestAwardKind`: `beaten-*` = zerado, `completed-*`/`mastered` =
/// 100% — sempre com o modo `-softcore`/`-hardcore`, exceto `mastered`).
#[derive(Debug, Clone, Copy)]
pub struct CompletionEntry {
    pub awarded: u32,
    pub max: u32,
    pub award: Option<Award>,
}

/// O prêmio que o RA dá a um jogo, decodificado do `HighestAwardKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Award {
    /// Chegou ao fim do jogo (a condição "beaten" do set).
    Beaten { hardcore: bool },
    /// 100% das conquistas.
    Completed { hardcore: bool },
    /// 100% em hardcore — o "mastered" clássico, sem sufixo de modo.
    Mastered,
}

/// A medalha do painel para um prêmio — pixel-art desenhada por
/// [`medal_rgba`], no estilo do app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Medal {
    /// 100% em hardcore (ou o "mastered" clássico).
    Gold,
    /// 100% em softcore — disco em contorno.
    GoldOutline,
    /// Zerou o jogo em hardcore.
    Silver,
    /// Zerou o jogo em softcore — disco em contorno.
    SilverOutline,
}

impl Award {
    /// A medalha do painel: disco dourado para 100% (cheio em hardcore,
    /// vazado em softcore) e prateado para "beaten"/zerou o jogo.
    pub fn medal(&self) -> Medal {
        match self {
            Award::Mastered | Award::Completed { hardcore: true } => Medal::Gold,
            Award::Completed { hardcore: false } => Medal::GoldOutline,
            Award::Beaten { hardcore: true } => Medal::Silver,
            Award::Beaten { hardcore: false } => Medal::SilverOutline,
        }
    }

    /// O prêmio em palavras, para as infos do painel (plan revision:
    /// "mostre se foi zerado e/ou mastered junto com as infos do game").
    pub fn label(&self) -> &'static str {
        match self {
            Award::Beaten { hardcore: true } => tr!("beaten (hardcore)", "zerado (hardcore)"),
            Award::Beaten { hardcore: false } => tr!("beaten (softcore)", "zerado (softcore)"),
            Award::Completed { hardcore: true } => {
                tr!("completed (hardcore)", "completado (hardcore)")
            }
            Award::Completed { hardcore: false } => {
                tr!("completed (softcore)", "completado (softcore)")
            }
            Award::Mastered => "mastered",
        }
    }

    /// Decodifica o `HighestAwardKind` do servidor — `None` para o que não
    /// for dos formatos conhecidos (o painel simplesmente omite a medalha).
    pub fn from_kind(kind: &str) -> Option<Award> {
        let (base, mode) = match kind.split_once('-') {
            Some((b, m)) => (b, Some(m)),
            None => (kind, None),
        };
        let hardcore = mode != Some("softcore");
        match base {
            "beaten" => Some(Award::Beaten { hardcore }),
            "completed" => Some(Award::Completed { hardcore }),
            "mastered" => Some(Award::Mastered),
            _ => None,
        }
    }
}

/// A medalha como RGBA (fita vermelha + disco; 11×14 px) — desenhada aqui
/// em código para nascer com o visual pixel-art do app e não depender de
/// asset nenhum. O disco do estilo `*Outline` é só o contorno (softcore).
pub fn medal_rgba(medal: Medal) -> (u32, u32, Vec<u8>) {
    const W: i32 = 11;
    const H: i32 = 14;
    let (base, hi, dark) = match medal {
        Medal::Gold | Medal::GoldOutline => ((255u8, 200u8, 40), (255, 236, 130), (176, 130, 16)),
        Medal::Silver | Medal::SilverOutline => {
            ((205u8, 205u8, 212), (245, 245, 248), (148, 148, 158))
        }
    };
    let outline = matches!(medal, Medal::GoldOutline | Medal::SilverOutline);
    let mut px = vec![0u8; (W * H * 4) as usize];
    let mut put = |x: i32, y: i32, c: (u8, u8, u8)| {
        let i = ((y * W + x) * 4) as usize;
        px[i..i + 4].copy_from_slice(&[c.0, c.1, c.2, 255]);
    };
    // A fita — três colunas com o friso central mais escuro.
    for y in 0..=3 {
        for x in 4..=6 {
            put(x, y, if x == 5 { (140, 26, 26) } else { (190, 40, 40) });
        }
    }
    // O disco, centrado logo abaixo: contorno escuro, brilho no quadrante
    // de cima à esquerda, base no resto — medalha de verdade tem volume.
    let (cx, cy, r) = (5.0f32, 8.6, 4.6);
    for y in 4..H {
        for x in 1..=9 {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let d2 = dx * dx + dy * dy;
            if d2 > r * r {
                continue;
            }
            let edge = d2 > (r - 1.2) * (r - 1.2);
            let c = match (outline, edge) {
                (true, false) => continue, // vazado: transparente no miolo
                (true, true) => dark,
                (false, true) => dark,
                (false, false) if dy < -0.6 && dx < 0.6 => hi,
                (false, false) => base,
            };
            put(x, y, c);
        }
    }
    (W as u32, H as u32, px)
}

/// Texture key estável por variante — mesmo padrão dos badges da estante
/// (`badge_image_id`), nunca colidindo com os ids derivados dos arquivos.
pub fn medal_image_id(medal: Medal) -> u64 {
    let name = match medal {
        Medal::Gold => "ra-medal-gold",
        Medal::GoldOutline => "ra-medal-gold-outline",
        Medal::Silver => "ra-medal-silver",
        Medal::SilverOutline => "ra-medal-silver-outline",
    };
    name.bytes().fold(0x9E37_79B9_7F4A_7C15u64, |mut acc, b| {
        acc ^= b as u64;
        acc = acc.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
        acc ^= acc >> 33;
        acc
    }) | (1 << 62)
}

/// O progresso do usuário em TODOS os jogos — uma chamada paginada. Este é
/// o substituto do `API_GetUserUnlocks` (morto na API atual — 404): o painel
/// só precisa de counts, e este endpoint entrega counts de todo o perfil de
/// uma vez. Sem ids de conquista individuais; o earned-set local segue como
/// estava (o servidor deduplica re-submissões de qualquer forma).
pub fn fetch_completion(
    user: &str,
    token: &str,
) -> Result<std::collections::HashMap<u64, CompletionEntry>, String> {
    let mut out = std::collections::HashMap::new();
    let mut offset = 0u32;
    loop {
        let resp = agent()
            .get(&format!(
                "{API_BASE}/API/API_GetUserCompletionProgress.php?u={}&y={}&c=500&o={offset}",
                encode(user),
                encode(token)
            ))
            .set("User-Agent", USER_AGENT)
            .call()
            .map_err(net_err)?;
        let body: serde_json::Value = read_json(resp).map_err(|e| format!("resposta: {e}"))?;
        let total = body.get("Total").and_then(|v| v.as_u64()).unwrap_or(0);
        let results = body
            .get("Results")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "resposta inesperada".to_string())?;
        for g in results {
            let Some(id) = g.get("GameID").and_then(|v| v.as_u64()) else {
                continue;
            };
            out.insert(
                id,
                CompletionEntry {
                    awarded: g.get("NumAwarded").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                    max: g.get("MaxPossible").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                    award: g
                        .get("HighestAwardKind")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .and_then(Award::from_kind),
                },
            );
        }
        offset += results.len() as u32;
        if results.is_empty() || offset as u64 >= total {
            break;
        }
    }
    Ok(out)
}

/// O (conquistadas, total, prêmio) do jogo a partir dos caches locais — o
/// earned e o prêmio vêm do completion do servidor quando disponível, senão
/// só o arquivo local (e sem prêmio: ele só existe no servidor). `hash` é o
/// hash RA da ROM, que o shelf já calculou na identificação.
pub fn tally_from_cache(
    hash: &str,
    completion: Option<&std::collections::HashMap<u64, CompletionEntry>>,
) -> Option<(usize, usize, Option<Award>)> {
    let game = cached_game(hash)?;
    let total = game.achievements;
    // Jogo identificado mas sem set publicado (0 conquistas reais): não há
    // tally — o painel não mostra "0 de 0" nem medalha (o botão Conquistas
    // some junto, no chamador).
    if total == 0 {
        return None;
    }
    let entry = completion.and_then(|c| cached_game_id(hash).and_then(|id| c.get(&id)));
    let earned = entry
        .map(|e| e.awarded as usize)
        .unwrap_or_else(|| load_earned(hash).len());
    Some((earned.min(total), total, entry.and_then(|e| e.award)))
}

/// Lançamento (ano) e editora do cache do RA — completa o painel para os
/// jogos que o DAT No-Intro/TOSEC não cobre. `(None, vazio)` quando não há
/// cache ou os campos vieram vazios.
pub fn release_and_extras(hash: &str) -> (Option<String>, Vec<(String, String)>) {
    let Some(text) = std::fs::read_to_string(cache_path(hash)).ok() else {
        return (None, Vec::new());
    };
    let Ok(body) = serde_json::from_str::<serde_json::Value>(&text) else {
        return (None, Vec::new());
    };
    let release = body
        .get("Released")
        .and_then(|v| v.as_str())
        .and_then(|s| s.get(..4))
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());
    let mut extras = Vec::new();
    if let Some(publisher) = body
        .get("Publisher")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        extras.push((
            tr!("publisher", "editora").to_string(),
            publisher.to_string(),
        ));
    }
    if let Some(developer) = body
        .get("Developer")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .filter(|d| Some(*d) != body.get("Publisher").and_then(|v| v.as_str()))
    {
        extras.push((
            tr!("developer", "desenvolvedor").to_string(),
            developer.to_string(),
        ));
    }
    if let Some(genre) = body
        .get("Genre")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        extras.push((tr!("genre", "gênero").to_string(), genre.to_string()));
    }
    (release, extras)
}

// ---- earned do servidor (os [x] da lista da estante) ------------------

/// Onde os ids ganhos no servidor ficam — `retroachievements/ra-cache/earned/<game_id>.json`.
fn earned_ids_path(game_id: u64) -> std::path::PathBuf {
    crate::dirs::retroachievements_dir()
        .join("ra-cache")
        .join("earned")
        .join(format!("{game_id}.json"))
}

/// Os ids de um fetch anterior que já estão no disco.
fn cached_earned_ids(game_id: u64) -> Option<EarnedMap> {
    std::fs::read_to_string(earned_ids_path(game_id))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
}

/// Os ids com `DateEarned`/`DateEarnedHardcore` não vazios no mapa
/// `Achievements` da resposta. Puro de propósito: o teste fixa o formato
/// real do `API_GetGameInfoAndUserProgress`.
fn earned_map_from_json(body: &serde_json::Value) -> EarnedMap {
    let mut out = EarnedMap::new();
    if let Some(map) = body.get("Achievements").and_then(|a| a.as_object()) {
        for (_k, a) in map {
            let Some(id) = a.get("ID").and_then(|v| v.as_u64()) else {
                continue;
            };
            let dated = |key: &str| {
                a.get(key)
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| !s.is_empty())
            };
            if dated("DateEarnedHardcore") {
                out.insert(id as u32, true);
            } else if dated("DateEarned") {
                out.insert(id as u32, false);
            }
        }
    }
    out
}

/// O que o servidor diz que o usuário já tem num jogo —
/// `API_GetGameInfoAndUserProgress`, o único endpoint vivo que devolve
/// `DateEarned` por conquista (`GetUserUnlocks`/`GetUserProgress` estão
/// mortos). Cacheado por game id: a estante nunca pergunta duas vezes.
pub fn fetch_game_earned(user: &str, token: &str, game_id: u64) -> Result<EarnedMap, String> {
    let resp = agent()
        .get(&format!(
            "{API_BASE}/API/API_GetGameInfoAndUserProgress.php?g={game_id}&u={}&y={}",
            encode(user),
            encode(token)
        ))
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(net_err)?;
    let body: serde_json::Value = read_json(resp).map_err(|e| format!("resposta: {e}"))?;
    let earned = earned_map_from_json(&body);
    let path = earned_ids_path(game_id);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(t) = serde_json::to_string(&earned) {
        let _ = polystnx_domain::fsutil::write_atomic(&path, t);
    }
    Ok(earned)
}

/// Une os ids ganhos no servidor no arquivo local (`ra-earned/<hash>.json`):
/// o que foi conquistado em outro lugar passa a valer aqui — aparece marcado
/// na lista e a sessão nunca re-submete. Devolve o set resultante.
fn merge_server_earned(hash: &str, server: &EarnedMap) -> EarnedMap {
    let mut all = load_earned(hash);
    let antes = (all.len(), all.values().filter(|&&hc| hc).count());
    for (&id, &hardcore) in server {
        match all.get(&id) {
            Some(true) => {}
            Some(false) if hardcore => {
                all.insert(id, true);
            }
            None => {
                all.insert(id, hardcore);
            }
            _ => {}
        }
    }
    if (all.len(), all.values().filter(|&&hc| hc).count()) != antes {
        save_earned(hash, &all);
    }
    all
}

/// Worker da estante: resolve o game id pelo cache da identificação (fase 2),
/// busca os ids ganhos (uma rede só na primeira vez) e devolve
/// `(hash, set já unido no arquivo local)`. `None` = jogo sem identificação
/// ou a busca falhou.
pub fn fetch_game_earned_worker(
    user: &str,
    token: &str,
    hash: &str,
) -> std::sync::mpsc::Receiver<(String, Option<EarnedMap>)> {
    let (tx, rx) = std::sync::mpsc::channel();
    let (user, token, hash) = (user.to_string(), token.to_string(), hash.to_string());
    std::thread::spawn(move || {
        let Some(game_id) = cached_game_id(&hash) else {
            let _ = tx.send((hash, None));
            return;
        };
        let server = match cached_earned_ids(game_id) {
            Some(ids) => ids,
            None => match fetch_game_earned(&user, &token, game_id) {
                Ok(ids) => ids,
                Err(e) => {
                    log::info!("ra earned {game_id}: {e}");
                    let _ = tx.send((hash, None));
                    return;
                }
            },
        };
        let merged = merge_server_earned(&hash, &server);
        let _ = tx.send((hash, Some(merged)));
    });
    rx
}

// ---- badges ----------------------------------------------------------

/// Nome de badge como veio do servidor, reduzido ao conjunto inofensivo
/// `[A-Za-z0-9_.-]` — o `BadgeName` do RetroAchievements alimenta tanto a
/// URL quanto o nome de arquivo do cache, e um valor hostil (`../..`)
/// não pode escapar do diretório de badges.
fn sanitize_badge_name(name: &str) -> Option<String> {
    let clean: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let clean = clean.replace("..", "_");
    (!clean.is_empty()).then_some(clean)
}

/// Where a downloaded badge lives — `retroachievements/ra-cache/badges/<name>.png`.
pub fn badge_path(name: &str) -> Option<std::path::PathBuf> {
    let name = sanitize_badge_name(name)?;
    let p = crate::dirs::retroachievements_dir()
        .join("ra-cache")
        .join("badges")
        .join(format!("{name}.png"));
    p.is_file().then_some(p)
}

/// Download a badge PNG once; later calls read the cache. Returns the path.
pub fn download_badge(name: &str) -> Option<std::path::PathBuf> {
    if name.is_empty() {
        return None;
    }
    if let Some(p) = badge_path(name) {
        return Some(p);
    }
    // O nome vem do servidor: sanitizado já para a URL, não só para o
    // arquivo do cache.
    let name = sanitize_badge_name(name)?;
    let resp = agent()
        .get(&format!("{API_BASE}/Badge/{name}.png"))
        .set("User-Agent", USER_AGENT)
        .call()
        .ok()?;
    // Badges são PNGs de poucos KB — um corpo gigante é coisa errada.
    let mut bytes = Vec::new();
    resp.into_reader()
        .take(2 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .ok()?;
    // Um badge que declara dimensões absurdas nem entra no cache.
    if !crate::imgutil::fits(&bytes, crate::imgutil::BADGE_MAX_SIDE) {
        log::warn!("ra: badge {name} recusado (não é uma imagem de até 512 px)");
        return None;
    }
    let dir = crate::dirs::retroachievements_dir()
        .join("ra-cache")
        .join("badges");
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(format!("{name}.png"));
    polystnx_domain::fsutil::write_atomic(&path, &bytes).ok()?;
    Some(path)
}

/// Baixa os badges de todas as conquistas do set (worker thread, fire and
/// forget) — a notificação de desbloqueio e a lista da estante encontram
/// tudo já em `retroachievements/ra-cache/badges/` na hora, sem custo de rede no
/// momento do uso. Chamado quando um jogo é identificado.
pub fn prefetch_badges(rom_path: &std::path::Path) {
    let rom_path = rom_path.to_path_buf();
    let run = move || {
        let Ok(hash) = hash_rom(&rom_path) else {
            return;
        };
        let Ok(text) = std::fs::read_to_string(cache_path(&hash)) else {
            return;
        };
        let Ok(body) = serde_json::from_str::<serde_json::Value>(&text) else {
            return;
        };
        let names: Vec<String> = parse_achievements(&body)
            .into_iter()
            .map(|a| a.badge)
            .filter(|n| !n.is_empty())
            .collect();
        let novos = names
            .iter()
            .filter(|n| badge_path(n).is_none())
            .filter(|n| download_badge(n).is_some())
            .count();
        if novos > 0 {
            log::info!("ra: {novos} badge(s) baixados");
        }
    };
    std::thread::spawn(run);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending_entry(id: u32, hardcore: bool) -> PendingAward {
        PendingAward {
            user: "UserA".into(),
            hash: "abc123".into(),
            id,
            hardcore,
            title: format!("Conquista {id}"),
            refusals: 0,
        }
    }

    fn pending_tmp(nome: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("polystnx-ra-pending-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("ra-pending.jsonl")
    }

    #[test]
    fn refused_pending_is_dropped_after_max_refusals() {
        let path = pending_tmp("refused");
        let a = pending_entry(9, false);
        pending_add(&path, &a).unwrap();
        for _ in 1..MAX_REFUSALS {
            pending_refused(&path, &a).unwrap();
            assert_eq!(read_pending(&path).len(), 1, "ainda na fila");
        }
        pending_refused(&path, &a).unwrap();
        assert!(read_pending(&path).is_empty(), "recusado demais: sai");
        assert!(is_transient_failure("rede: Dns"));
        assert!(is_transient_failure("HTTP 503"));
        assert!(!is_transient_failure("Unknown achievement"));
        // Token revogado: 401 com JSON — credencial, não recusa do envio.
        assert!(is_transient_failure(
            "HTTP: Invalid user/token combination."
        ));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn pending_add_remove_round_trip() {
        let path = pending_tmp("roundtrip");
        let (a, b) = (pending_entry(1, false), pending_entry(2, true));
        pending_add(&path, &a).unwrap();
        pending_add(&path, &b).unwrap();
        // O mesmo envio não duplica; outro modo é outro envio.
        pending_add(&path, &a).unwrap();
        let hc = pending_entry(1, true);
        pending_add(&path, &hc).unwrap();
        assert_eq!(read_pending(&path), vec![a.clone(), b.clone(), hc.clone()]);

        pending_remove(&path, &a).unwrap();
        assert_eq!(read_pending(&path), vec![b.clone(), hc.clone()]);
        pending_remove(&path, &b).unwrap();
        pending_remove(&path, &hc).unwrap();
        assert!(read_pending(&path).is_empty());
        // Remover o que não está lá não falha.
        pending_remove(&path, &a).unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn pending_skips_malformed_lines_and_reads_legacy_tsv() {
        let path = pending_tmp("malformed");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let good = pending_entry(7, true);
        let text = format!(
            "{}\nlixo\n{{\"hash\": 1}}\n\nabc123\tnão-número\t1\tx\nfff\t42\t0\tLegada\twith tab\n",
            serde_json::to_string(&good).unwrap()
        );
        std::fs::write(&path, text).unwrap();
        let legacy = PendingAward {
            user: String::new(),
            hash: "fff".into(),
            id: 42,
            hardcore: false,
            title: "Legada\twith tab".into(),
            refusals: 0,
        };
        assert_eq!(read_pending(&path), vec![good.clone(), legacy.clone()]);

        // A regravação mantém só as válidas, já no formato JSON.
        pending_remove(&path, &good).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert_eq!(raw.lines().count(), 1);
        assert!(raw.starts_with('{'));
        assert_eq!(read_pending(&path), vec![legacy]);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn already_awarded_counts_as_success() {
        assert!(is_already_awarded(
            "User already has this achievement unlocked in hardcore mode."
        ));
        assert!(!is_already_awarded("Invalid token"));
    }

    #[test]
    fn award_signature_is_md5_of_id_user_hardcore() {
        // md5("1234UserA1")
        assert_eq!(award_signature(1234, "UserA", true).len(), 32);
        assert_ne!(
            award_signature(1234, "UserA", true),
            award_signature(1234, "UserA", false)
        );
    }

    #[test]
    fn encode_leaves_safe_chars_and_escapes_the_rest() {
        assert_eq!(encode("user.name-1_~"), "user.name-1_~");
        assert_eq!(encode("a b&c=d"), "a%20b%26c%3Dd");
    }

    #[test]
    fn empty_credentials_fail_before_any_network() {
        assert!(test_login("", "token").is_err());
        assert!(test_login("user", "").is_err());
    }

    #[test]
    fn parse_skips_hashed_memaddr_and_reads_both_shapes() {
        // Cache legado (rota Extended): MemAddr chega como hash sem lógica —
        // nenhuma conquista é ativada a partir disso.
        let legacy = serde_json::json!({
            "ID": 373, "Title": "Aero the Acro-Bat",
            "Achievements": { "10807": { "ID": 10807, "Title": "High Score",
                "MemAddr": "717146003ab7995fa293769ad320ee6e", "Points": 25 } }
        });
        assert!(parse_achievements(&legacy).is_empty());
        // Connect, shape plano: MemAddr com a lógica de verdade.
        let flat = serde_json::json!({
            "GameID": 373, "Title": "Aero the Acro-Bat",
            "Achievements": { "10807": { "ID": 10807, "Title": "High Score",
                "MemAddr": "0xH0042=10", "Points": 25, "BadgeName": "199825" } }
        });
        let parsed = parse_achievements(&flat);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].memaddr, "0xH0042=10");
        assert_eq!(game_from_json(&flat).unwrap().achievements, 1);
        // Connect, shape multiset: sets com arrays de conquistas.
        let multi = serde_json::json!({
            "Success": true, "Sets": [ {
                "AchievementSetId": 1, "GameId": 373, "Title": "Aero the Acro-Bat",
                "Achievements": [ { "ID": 10807, "Title": "High Score",
                    "MemAddr": "0xH0042=10", "Points": 25 } ] } ]
        });
        assert_eq!(parse_achievements(&multi).len(), 1);
        let g = game_from_json(&multi).unwrap();
        assert_eq!(g.title, "Aero the Acro-Bat");
        assert_eq!(g.achievements, 1);
        assert!(!g.unsupported);
    }

    #[test]
    fn warning_set_reads_as_unsupported() {
        // Hash marcado "Unsupported Game Version" no RA (caso do Batman
        // Forever Rev 1): a resposta é o set fictício de aviso — 0
        // conquistas reais e o botão Conquistas desativa com motivo.
        let dummy = serde_json::json!({
            "Success": true,
            "Title": "Unsupported Game Version (Batman Forever)",
            "Sets": [ { "Title": null, "Type": "core", "AchievementSetId": 407,
                "GameId": 1100000418,
                "Achievements": [ { "ID": 101000001, "MemAddr": "1=1.300.",
                    "Title": "Unsupported Game Version", "Points": 0,
                    "BadgeName": "00000" } ] } ]
        });
        let g = game_from_json(&dummy).unwrap();
        assert!(g.unsupported);
        assert_eq!(g.achievements, 0);
        // E nada do aviso vira conquista ativável.
        assert!(parse_achievements(&dummy).is_empty());
    }

    #[test]
    fn client_warning_in_normal_set_is_not_unsupported() {
        // Cliente fora do registro oficial: o aviso é INJETADO em respostas
        // normais também — jogo com conquistas reais NÃO é "sem suporte".
        let body = serde_json::json!({
            "Success": true, "Title": "Aero the Acro-Bat",
            "Sets": [ { "Achievements": [
                { "ID": 101000001, "MemAddr": "1=1.300.", "Title": "Atenção",
                    "Points": 0, "BadgeName": "00000" },
                { "ID": 10807, "MemAddr": "0xH0042=10", "Title": "High Score",
                    "Points": 25 } ] } ]
        });
        let g = game_from_json(&body).unwrap();
        assert!(!g.unsupported);
        assert_eq!(g.achievements, 1);
    }

    #[test]
    fn identified_with_empty_set_is_not_unsupported() {
        // Jogo com entrada no RA mas SEM set publicado (caso Spawn: The
        // Eternal): identificado, zero conquistas reais, e NÃO é "versão sem
        // suporte" — é o formato que mantém o botão Conquistas fora do
        // painel e o tally fora das infos (sem "0 de 0").
        let body = serde_json::json!({
            "Success": true, "Title": "Spawn: The Eternal",
            "Sets": [ { "AchievementSetId": 1, "GameId": 18829,
                "Achievements": [] } ]
        });
        let g = game_from_json(&body).unwrap();
        assert_eq!(g.title, "Spawn: The Eternal");
        assert_eq!(g.achievements, 0);
        assert!(!g.unsupported);
    }

    #[test]
    fn award_kinds_decode_and_map_to_medals() {
        // Os formatos que o servidor manda no HighestAwardKind (verificado
        // na conta real: null e "beaten-softcore" são os presentes hoje).
        assert_eq!(
            Award::from_kind("beaten-softcore"),
            Some(Award::Beaten { hardcore: false })
        );
        assert_eq!(
            Award::from_kind("beaten-hardcore"),
            Some(Award::Beaten { hardcore: true })
        );
        assert_eq!(
            Award::from_kind("completed-hardcore"),
            Some(Award::Completed { hardcore: true })
        );
        assert_eq!(
            Award::from_kind("completed-softcore"),
            Some(Award::Completed { hardcore: false })
        );
        // O mastered clássico vem sem sufixo de modo.
        assert_eq!(Award::from_kind("mastered"), Some(Award::Mastered));
        // Desconhecido/nulo: sem medalha no painel.
        assert_eq!(Award::from_kind("???"), None);
        // E o mapeamento para a medalha: ouro = 100% (cheio no hardcore,
        // vazado no softcore), prata = zerou.
        assert_eq!(
            Award::from_kind("mastered").map(|a| a.medal()),
            Some(Medal::Gold)
        );
        assert_eq!(
            Award::from_kind("completed-hardcore").map(|a| a.medal()),
            Some(Medal::Gold)
        );
        assert_eq!(
            Award::from_kind("completed-softcore").map(|a| a.medal()),
            Some(Medal::GoldOutline)
        );
        assert_eq!(
            Award::from_kind("beaten-hardcore").map(|a| a.medal()),
            Some(Medal::Silver)
        );
        assert_eq!(
            Award::from_kind("beaten-softcore").map(|a| a.medal()),
            Some(Medal::SilverOutline)
        );
    }

    #[test]
    fn medal_is_an_11x14_sprite_with_ribbon_and_disc() {
        let (w, h, px) = medal_rgba(Medal::Gold);
        assert_eq!((w, h), (11, 14));
        let at = |x: u32, y: u32| {
            let i = ((y * w + x) * 4) as usize;
            (px[i], px[i + 1], px[i + 2], px[i + 3])
        };
        // Canto fora de tudo: transparente.
        assert_eq!(at(0, 13), (0, 0, 0, 0));
        // Fita vermelha no topo.
        assert_eq!(at(4, 1), (190, 40, 40, 255));
        // Miolo do disco dourado, opaco.
        assert_eq!(at(5, 9), (255, 200, 40, 255));
        // A variante vazada deixa o miolo transparente e mantém o anel.
        let (_, _, hollow) = medal_rgba(Medal::SilverOutline);
        let center = (9 * 11 + 5) * 4;
        assert_eq!(hollow[center + 3], 0);
        let ring = (8 * 11 + 1) * 4; // borda esquerda do disco, meio da altura
        assert_eq!(hollow[ring + 3], 255);
        // Ids estáveis e distintos por variante.
        assert_ne!(medal_image_id(Medal::Gold), medal_image_id(Medal::Silver));
        assert_eq!(medal_image_id(Medal::Gold), medal_image_id(Medal::Gold));
    }

    #[test]
    fn points_come_as_number_or_string() {
        // O cache da identificação traz número JSON; o endpoint de progresso,
        // string. Os dois têm de valer.
        assert_eq!(points_from_json(&serde_json::json!({"Points": 5})), 5);
        assert_eq!(points_from_json(&serde_json::json!({"Points": "10"})), 10);
        assert_eq!(points_from_json(&serde_json::json!({})), 0);
    }

    #[test]
    fn earned_ids_come_from_date_earned_fields() {
        // Formato real do API_GetGameInfoAndUserProgress (Tekken 3, conta
        // com 4 ganhas): DateEarned para softcore, DateEarnedHardcore para
        // hardcore; quem não tem traz a chave nula ou nem traz.
        let body = serde_json::json!({
            "ID": 11259,
            "Title": "Tekken 3",
            "Achievements": {
                "95922": {"ID": 95922, "Title": "Start of Something Greater",
                          "DateEarned": "2024-01-01 00:00:00", "DateEarnedHardcore": null},
                "95999": {"ID": 95999, "Title": "Seven Gold Letters",
                          "DateEarnedHardcore": "2024-01-02 00:00:00"},
                "96023": {"ID": 96023, "Title": "Legs of Steel", "DateEarned": null},
                "96025": {"ID": 96025, "Title": "Combo Finish!", "DateEarned": null}
            }
        });
        let earned = earned_map_from_json(&body);
        // Softcore (só DateEarned) vira false; hardcore vira true.
        assert_eq!(earned.get(&95922), Some(&false));
        assert_eq!(earned.get(&95999), Some(&true));
        assert_eq!(earned.len(), 2);
        assert!(earned_map_from_json(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn game_json_parses_title_and_achievement_count() {
        let body: serde_json::Value = serde_json::json!({
            "Title": "Chrono Trigger",
            "Achievements": {
                "1": {"ID": 1, "Title": "a"},
                "2": {"ID": 2, "Title": "b"},
                "3": {"ID": 3, "Title": "c"}
            }
        });
        let g = game_from_json(&body).unwrap();
        assert_eq!(g.title, "Chrono Trigger");
        assert_eq!(g.achievements, 3);
        assert!(game_from_json(&serde_json::json!({})).is_none());
    }

    /// Hits the real API — manual sanity check only:
    /// `cargo test -p polystnx-app --lib ra -- --ignored`
    #[test]
    #[ignore]
    fn bad_token_is_rejected_by_the_real_api() {
        let r = test_login(
            "definitely-not-a-real-user-xyz",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        );
        assert!(r.is_err());
    }
}
