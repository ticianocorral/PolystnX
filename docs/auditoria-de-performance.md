# Auditoria de Performance — PSX Xperience

**Data:** 2026-10-03
**Escopo:** runtime do app (loop de UI, renderização, worker do core, áudio, entradas), revisão estática do código em `crates/platform` e `crates/app`.
**Método:** análise do código dos caminhos quentes + inventário das otimizações já presentes. Ferramentas de medição embutidas listadas no fim.

---

## 1. Arquitetura de runtime (resumo)

- **Thread de UI** (`runner`): loop a ~60 fps (`pace_frame`), bomba de eventos SDL, desenho da cena completa a cada frame (`present_frame`/`present_static`), áudio enfileirado por quadro.
- **Thread do core** (`swanstation`): um `Run` em voo por vez (`in_flight`), frame + áudio + RAM devolvidos por canal; prioridade baixa no macOS.
- **Renderização:** SDL3 + GPU. O quadro do jogo sobe por textura (`upload`) e passa pelo warp CRT (`build_crt_mesh`); a interface (gabinete, painel) é desenhada por cima com `fill_rect`/`copy` por glifo.

O frame budget a 60 fps é ~16,6 ms; o trace embutido acusa frames atrasados quando o loop estoura (`pace_frame` + `PSX_XPERIENCE_TRACE=1`).

---

## 2. Otimizações já presentes (inventário)

| Onde | O quê |
|---|---|
| `present_frame` | Mesh CRT e bezel **em cache** (`ensure_mesh`/`ensure_bezel` com chave de tamanho) — a reconstrução de 1.089 vértices por frame "was pure waste" (removida). |
| `present_static` | Ruído da estática regenerado a ~30 Hz (não a 60) e só quando o nível muda (`noise_stamp`). |
| Comandos do painel | Legendas reconstruídas só quando a assinatura muda (`prev_sig`: flash, tampa, disco) — era "churn (perf pass)". |
| Gamepads | Re-enumerados só em hotplug (`devices_pending`), não por frame. |
| Áudio | Fila limitada a ~0,15 s (`audio_cap`) — evita latência crescente; `queue` é `put_data_i16` direto, sem conversão. |
| Giro do disco | Por relógio com rampa (`Instant::now` 1×/frame), não por frame emulado. |
| Texto | Avanço inteiro por glifo (1 `copy` por letra, **mesma textura-atlas** → lote único no render). |
| RA | Tick avalia contra cópia `Arc` da RAM feita pelo worker antes do run-ahead. |
| Capturas | `read_pixels` só em caminhos de captura/debug — nunca no loop. |

---

## 3. Achados

### A1 — Cópia integral do quadro por frame (`LastFrame::from_frame`) — **MÉDIO — ✅ IMPLEMENTADO (2026-10-03)**

`runner.rs` (~1983, ~2020, ~3468): todo quadro recebido do core passa por
`LastFrame::from_frame(frame)`, que **aloca e converte/copía o frame inteiro**
(~640×478×4 ≈ 1,2 MB; conversão por pixel quando RGB565) **duas vezes na pior
hipótese** (uma no drain do `in_flight`, outra no quadro apresentado). O custo
dobra o trabalho necessário: o `upload()` da plataforma copia os mesmos pixels
de novo para a textura.

- Consumidores reais: `fref` (empréstimo para o `present_frame`) e
  `last_render` (repaint em resize/saída).
- **Sugestão:** para XRGB8888, enviar o `&frame` direto ao upload (zero-cópia)
  e manter a cópia CPU só quando `Rgb565` (conversão necessária). Alternativa
  maior: reter a **textura** já carregada em vez da cópia CPU (repaint por
  `render_geometry`, sem memcpy).

### A2 — Cópia de 2 MB de RAM por frame com sessão RA ativa — **MÉDIO (condicionado) — ✅ IMPLEMENTADO (2026-10-03)**

`CoreOut.ram` (`runner.rs` ~409): enquanto há sessão RA logada, o worker faz
`core.memory(MEMORY_SYSTEM_RAM)` → `to_vec()` de **2 MB por frame**
(≈ 120 MB/s de alocação + cópia) para o tick de conquistas rodar na main.
- **Sugestão:** mapear o ponteiro da RAM uma única vez (`retro_get_memory_data`
  é estável durante o jogo) e o tick ler direto sob o fence do `in_flight`; ou
  amortizar o tick do RA (avaliar a cada N quadros — as condições de conquista
  são de baixa frequência).

### A3 — `composite_screen` rebuilda o mesh CRT a cada frame no caminho 2D — **BAIXO — ✅ IMPLEMENTADO (2026-10-03)**

`cabinet.rs` ~3064: a estante/configurações/idle-2d chamam
`build_crt_mesh(self.screen, 1.0)` **por frame** (1.089 vértices + índices,
alocados e descartados). O caminho de gameplay usa cache (`ensure_mesh`) — o
2D não.
- **Sugestão:** mesmo cache por `(w, h)`; o `screen` só muda em resize.

### A4 — Redesenho integral do painel por frame — **BAIXO (por design)**

`draw_panel` + `draw_slot_furniture` emitem ~150–250 chamadas de desenho por
frame (fill_rects das placas/biséis + glifos dos rótulos/comandos). Com o
console **ligado** o giro do disco invalida o quadro de qualquer forma. Com o
console **desligado** (estática + painel estático) tudo poderia ir para uma
textura em cache e ser invalidado por dirty-flag.
- **Sugestão (se o trace acusar):** cache do painel desligado em textura;
  invalidar em `set_*`. Não priorizar antes de medição.

### A5 — Fonte: um `copy` por glifo — **INFO**

~60–200 glifos/frame no painel (62 chamadas de texto no arquivo). Todos usam a
mesma atlas → o SDL agrupa por textura; o custo é contado em chamadas, não em
uploads. Em resoluções altas (2560) o total cresce linearmente com o texto.
- **Sugestão (futuro):** atlas de page-cache por rótulo estático se o trace
  apontar o painel.

### A6 — `draw_modal` reconstrói os botões por frame — **INFO**

Só enquanto um modal está aberto (transitório). Ok.

### A7 — Áudio — **OK**

Queue direta (`put_data_i16`), cap de 0,15 s, sem conversão por amostra; o
`tick_cd_noise` usa fade curto e amostras embutidas. Nada a fazer.

### A8 — Entradas — **OK**

`sample_gamepads` por frame é O(botões×portas) trivial; hotplug re-sincroniza
só em eventos. `MouseMove` agora flui pelo `poll` (uma entrada de evento por
movimento) — o custo é o do `window_to_output` (2 ints) enquanto o arrasto do
controle está ativo; fora dele, o braço sai cedo.

---

## 4. Steam Deck (considerações)

- Tela 1280×800: o canvas do gabinete fica **sem offset** (aspecto dentro do
  clamp 16:10–16:9? — verificar em runtime; o offset do viewport só existe
  acima de 16:9) e o custo dominante é o **upload do quadro** + warp.
- O par `runahead = 0` padrão já evita o pior caso (save/load de estado do
  SwanStation por frame).
- O véu do modal (`fill_rect_rgba` alpha 120) e as placas das entradas são
  baratos.

---

## 5. Como medir

| Ferramenta | O quê |
|---|---|
| `PSX_XPERIENCE_TRACE=1` | Spans de `poll`/`poll_menu`/`poll_text_entry`/`present` + aviso **"frame atrasado X ms"** quando o loop estoura o budget. |
| `PSX_XPERIENCE_DEBUG_COMPOSITE=<dir>` | Dump do composto real (o que a Metal desenhou) nos frames 5/30/300 do modal. |
| Log `DEBUG ensure_src` | Recreação de textura do quadro (deve acontecer 1× por sessão, não por frame). |
| `cargo run --release` + Instruments (Time Profiler) | Confirmar A1 (cópia de frame) e A2 (RAM RA) na aba de allocs. |

**Procedimento sugerido para cada achado:** medir com TRACE=1 em cena-alvo
(jogo rodando, modal aberto, idle), aplicar a correção, re-medir o
"frame atrasado" máximo em 60 s.

---

## 6. Prioridades

1. ~~**A1** — eliminar a cópia dupla do quadro~~ ✅ XRGB8888 empresta os
   pixels do core direto ao upload (zero-cópia); RGB565 converte uma vez no
   buffer reutilizado; `last_frame` (Vec reutilizado) alimenta o
   "reapresenta"/shot.
2. ~~**A2** — RAM do RA~~ ✅ `CoreOut.ram` agora carrega (ponteiro, tamanho)
   do `memory_ptr` novo; o tick lê a RAM viva sob o fence do worker (bloqueado
   no recv), com o comentário SAFETY no local.
3. ~~**A3** — cache do mesh no caminho 2D~~ ✅ `mesh_2d` em cache nos 3 sites
   (estante/config/idle-2d/composite), `take` + devolve como o mesh do
   gameplay.
4. **A4** — só com evidência do trace em hardware alvo (Steam Deck).
