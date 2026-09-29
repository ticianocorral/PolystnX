# Avisos de terceiros

Este projeto **não distribui** nenhum core de emulação, ROM, BIOS ou arte. As
notas abaixo se aplicam quando você monta um binário que carrega esses
componentes.

## SwanStation (core libretro)

O caminho de emulação usa o core `swanstation_libretro`
([SwanStation](https://github.com/libretro/swanstation), herdeiro do
DuckStation), licenciado sob **GPL v2+** (confirme a variante no repositório
do core na hora de publicar). O app **não distribui** o core — baixa-o do
buildbot oficial do libretro pelo próprio menu de configurações; ao publicar
um binário que baixe o core, inclua o texto da GPL junto.

O app também **não distribui BIOS de console** — o arquivo `SCPH*.BIN` é
fornecido por quem roda, em `bios/`, e nunca é baixado nem embarcado.

## rcheevos (MIT) e zlib do crate `chd`

`crates/ra/vendor/rcheevos/` é o snapshot *develop* do
[rcheevos](https://github.com/RetroAchievements/rcheevos) (MIT), compilado
com o `rhash` de disco (sem LUA, sem zip, sem imagens criptografadas). O
parsing de CHD do hashing é feito em Rust pelo crate
[`chd`](https://crates.io/crates/chd) (MIT/Apache), que puxa
[`flate2`](https://crates.io/crates/flate2) (MIT/Apache) para as compressões
zlib/deflate do formato.

## SDL3 (zlib)

A camada de plataforma usa [SDL3](https://www.libsdl.org/), sob a licença zlib.

## snes_ntsc (LGPL v2.1+) — presente, sem consumidor

`crates/ntsc/vendor/snes_ntsc/` contém `snes_ntsc` 0.2.2 de Shay Green (blargg),
herdado do irmão de SNES (LGPL v2.1+). O PSX nasce **sem** filtro composto
(plano §2) — o crate compila, mas nenhum frame passa por ele. Removê-lo do
workspace é limpeza futura; enquanto estiver lá, o aviso LGPL fica.

## libretro-database — pasta `cht` (Fase 4, revisão)

`crates/domain/src/cheats_data.txt` embute a pasta `cht` inteira do
[`libretro-database`](https://github.com/libretro/libretro-database) pro
SNES — `github.com/libretro/libretro-database/tree/master/cht/Nintendo%20-%20Super%20Nintendo%20Entertainment%20System`,
gerado por `scripts/gen_cheats_data.py` (revisão: antes era uma lista
curada de ~20 jogos escolhidos a dedo; agora é a base toda, ~2400 jogos,
pra cobrir qualquer ROM que o jogador adicionar, não só as poucas
testadas na hora). (O plano, §4.4, citava MIT de memória — o `LICENSE` do
repositório é na verdade **CC BY-SA 4.0**; corrigido aqui.)

Diferente da revisão anterior deste arquivo: desta vez tanto o código de
cada cheat (endereço/valor, fato bruto sem expressão autoral — do jeito
que uma lista de números de telefone não vira obra protegida por estar
arrumada numa tabela) quanto a **descrição** são reproduzidos como estão
na base, sem reescrever — na escala de milhares de linhas, reescrever
cada uma não seria viável nem teria sentido (são rótulos factuais curtos,
não prosa). Por isso `cheats_data.txt` em si é distribuído sob a mesma
licença da base (CC BY-SA 4.0 — o *share-alike* da licença já pede isso
de qualquer coleção derivada; ver `scripts/gen_cheats_data.py` pra como
foi gerado, prova de que não é uma cópia opaca).

> Cheats (códigos e descrições) adaptados de `libretro-database`
> (github.com/libretro/libretro-database), licenciado sob
> [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/).
> Copyright dos respectivos colaboradores do projeto libretro.
> `crates/domain/src/cheats_data.txt` é distribuído sob os mesmos termos.

## TOSEC — ano/editora embutidos (Fase 4, revisão)

`crates/domain/src/tosec_data.txt` embute um recorte do datfile SNES
"Games" do [TOSEC](https://www.tosecdev.org/) (The Old School Emulation
Center), gerado por `scripts/gen_tosec_data.py`. Só duas informações são
guardadas por CRC32: **ano** e **editora** — extraídas do nome que o
TOSEC dá a cada jogo (o formato usa parênteses, ex.
`"Chrono Trigger (1995)(Square)(US)"`), não copiadas do dat de outra
forma. O TOSEC não tem campos `<year>`/`<publisher>` próprios como um dat
Logiqx/No-Intro — o nome catalogado É a única fonte dessas duas datas.

Diferente da base de cheats acima, o TOSEC **não publica uma licença
explícita** para o conteúdo dos seus datfiles (o GPL mencionado no site é
da ferramenta de CMS Joomla, não dos dados). Por isso, só os dois fatos
nus (um ano, um nome de editora) são embutidos — nunca o nome catalogado
do TOSEC nem sua descrição completa; o título mostrado na estante e no
painel do app continua vindo do cabeçalho da própria ROM ou de um DAT
No-Intro opcional, exatamente como antes. Um DAT No-Intro do usuário, se
carregado, sempre tem prioridade sobre esse ano/editora embutido quando
os dois concordam em ter a mesma informação (`catalog.rs::info_lines`).

**Cobertura ampliada via DAT-o-MATIC (No-Intro), sem embutir nada dele.**
Um dump específico (ex. uma revisão "(Rev 1)") às vezes não bate com
nenhuma entrada catalogada pelo TOSEC por CRC32, mesmo quando o TOSEC
claramente conhece o jogo sob outro dump. `scripts/gen_tosec_data.py`
aceita opcionalmente o dat oficial do SNES do
[No-Intro](https://datomatic.no-intro.org/) (baixado manualmente pelo
site — sem link direto, mesma situação de sempre) só para ler a relação
`id`/`cloneofid` de cada `<game>` — "estes CRC32s são revisões/regiões do
mesmo jogo" — e propagar o ano/editora já extraído do TOSEC para outros
CRC32s da mesma família que o TOSEC não catalogou sozinho. Nenhum nome,
descrição ou outro texto do No-Intro é lido para o arquivo final; o valor
gravado continua sendo 100% o ano/editora que o TOSEC forneceu para
algum membro da família, só aplicado a mais hashes.



## Efeitos sonoros do console (Pixabay)

Os sons de inserir/ejetar cartucho e ligar/desligar/resetar (`crates/app/src/
sfx/*.wav`, embutidos no binário) são recortes de efeitos do usuário
[u_fom5qo8e5o](https://pixabay.com/users/u_fom5qo8e5o-48608561/) no Pixabay,
usados sob a [Pixabay Content License](https://pixabay.com/service/license-summary/)
(grátis para uso comercial, sem atribuição exigida — os créditos aqui são por
cortesia):

- "SNES cartridge insert" — pixabay.com/sound-effects/film-special-effects-snes-cartridge-insert-296151/
- "SNES Eject" — pixabay.com/sound-effects/film-special-effects-snes-eject-296153/
- "SNES power on" — pixabay.com/sound-effects/film-special-effects-snes-power-on-296158/
- "SNES POwer off" — pixabay.com/sound-effects/film-special-effects-snes-power-off-296155/
- "SNES reset" — pixabay.com/sound-effects/snes-reset-296152/

Os arquivos foram aparados (silêncio inicial removido, fade de ~120 ms no
corte) e convertidos para WAV mono 22 050 Hz; nenhum outro ajuste.



## Marcas

"Super Nintendo", "Super Famicom", "SNES" e o trade dress do console são marcas
da Nintendo. O console na cena é "inspirado em", não uma réplica (plano §7).

## rcheevos (runtime de conquistas)

`crates/ra/vendor/rcheevos/` embute um snapshot do
[rcheevos](https://github.com/RetroAchievements/rcheevos) (develop), licenciado
sob **zlib** — só o avaliador de condições (`rcheevos/`) e utilitários
(`rc_compat`, `rc_util`, `md5`); sem cliente completo, sem hashing de disco.
Compilado por `crates/ra/build.rs` com `RC_DISABLE_LUA` e `RC_HASH_NO_DISC`.
O texto integral da licença acompanha o repositório vendido.

## RetroAchievements (serviço)

Conquistas, badges e dados de jogos são © [RetroAchievements](https://retroachievements.org)
e seus usuários; o app só faz cache local (`saves/ra-cache/`) do que a conta
do próprio jogador acessa, identifica-se via User-Agent próprio e usa o token
de Web API gerado pelo próprio usuário.

## lopdf (crate Rust)

Parser de PDF usado pelo leitor de manuais (`crates/app/src/manual.rs`):
[lopdf](https://github.com/J-F-Liu/lopdf), licenciado sob **MIT** — o app
apenas extrai as imagens de páginas embutidas dos PDFs que o próprio
usuário coloca em `assets/manual/`; nada de PDF é distribuído com o app.
