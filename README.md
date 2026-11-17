# Novo Messenger

Mensagens sem internet, sem sinal de operadora e sem créditos. O mesmo núcleo em **Rust** roda no navegador (WebAssembly) e no terminal (binário nativo), e manda texto por três meios ao mesmo tempo:

| Canal | Como funciona | Alcance | Precisa de algo extra? |
|---|---|---|---|
| **Luz** | A tela (ou a lanterna no Android) pisca os bits; o outro aparelho lê com a câmera | 10–30 cm, de frente | Não |
| **Som** | Dois tons (FSK) pelo alto-falante; o outro aparelho escuta pelo microfone. Há um modo quase inaudível (17,5/18,5 kHz) | 1–2 m | Não |
| **LoRa** | Conecta por Bluetooth a um rádio [Meshtastic](https://meshtastic.org) e manda pelo rádio | vários km | Rádio LoRa (~R$ 100–200) |

Marque mais de um canal e a mensagem sai por todos. O receptor junta as cópias repetidas.

## Arquitetura

```
             ┌──────────────────────── nm-core (Rust, sem dependências) ─────────────────────────┐
 câmera ───► │ light: limiar adaptativo (filas monotônicas, O(1))  ┐                              │
             │                                                      ├► uart ► frame ► session ──► eventos
 microfone ► │ fsk: Goertzel + janela de Hann, tempo pelo relógio   ┘   bits   STX/CRC   fases 1–4    │
             │      de amostras                                                                   │
             │ fsk::synth: áudio com fase contínua   ◄── tx_bits (toggle · bytes de teste · texto)  │
             │ mesh: protobuf Meshtastic (texto)                                                  │
             └─────────────┬──────────────────────────────────────────────┬───────────────────────┘
                           │                                              │
        nm-wasm (C ABI, 69 KB, zero imports)               nm-cli (binário nativo `nm`)
                           │                                              │
     web/ (PWA: tela, câmera, microfone, Web Bluetooth)    terminal: WAV ou PCM ao vivo via
     Android · iOS · Windows · macOS · Linux               arecord/aplay/sox — Linux, macOS, Windows
```

- **Uma única implementação do protocolo.** O JavaScript só cuida da tela e dos sensores. Toda decisão de bit, byte, quadro e fase acontece no Rust, e é testada uma vez para todas as plataformas.
- **Sem dependências externas.** Nem `wasm-bindgen`: o módulo exporta funções C e não importa nada do navegador.
- **Funciona offline.** Depois da primeira visita, o service worker guarda o app e o `.wasm` no aparelho.

## Estrutura

```
Cargo.toml                 workspace
crates/nm-core/            protocolo e processamento de sinal (biblioteca)
  src/frame.rs             quadro STX + texto + checksum + ETX
  src/uart.rs              bits ↔ bytes; detector da fase 1
  src/light.rs             brilho da câmera → 0/1
  src/fsk.rs               áudio → 0/1 (Goertzel) e bits → áudio
  src/session.rs           cadeia completa de recepção e fases de teste
  src/mesh.rs              mensagens de texto do Meshtastic
crates/nm-wasm/            ponte WebAssembly (C ABI)
crates/nm-cli/             programa de terminal `nm`
web/                       app (index.html, app.js, nm.js, mic-worklet.js, sw.js)
build.sh                   compila o WebAssembly para web/nm_core.wasm
.github/workflows/         testes + publicação no GitHub Pages
```

## Compilar e rodar

Pré-requisito: [Rust](https://rustup.rs) estável.

```sh
# testes (núcleo + terminal)
cargo test --workspace

# WebAssembly → web/nm_core.wasm
./build.sh

# app no navegador (no computador; câmera e microfone funcionam em localhost)
python3 -m http.server 8080 -d web      # abra http://localhost:8080

# terminal (Linux, macOS, Windows)
cargo build --release -p nm-cli         # gera target/release/nm (nm.exe no Windows)
```

### Usar o `nm` no terminal

```sh
nm falar "Corte às 15h" -o msg.wav       # gera o som
nm ouvir msg.wav                         # decodifica
nm fases -o fases.wav                    # as 4 fases de teste em sequência

# ao vivo no Linux
nm falar "oi" | aplay -f FLOAT_LE -r 48000 -c 1
arecord -f FLOAT_LE -r 48000 -c 1 -t raw | nm ouvir -

# ao vivo no macOS ou Windows (com sox)
nm falar "oi" | sox -t f32 -r 48000 -c 1 - -d
sox -d -t f32 -r 48000 -c 1 - | nm ouvir -
```

Um computador com o `nm` conversa por som com um celular rodando o app, desde que usem o mesmo `--bit` e as mesmas frequências.

### Celular (Android e iOS)

1. Publique no GitHub Pages: em Settings → Pages, escolha **GitHub Actions** como origem. A automação em `.github/workflows/pages.yml` testa, compila o WebAssembly e publica a pasta `web/`.
2. Abra o link no celular **uma vez com internet** e use "Adicionar à tela inicial".
3. Depois disso o app abre sem rede.

A câmera, o microfone e o Bluetooth exigem HTTPS, e o GitHub Pages já usa HTTPS. Web Bluetooth (LoRa) funciona no Chrome do Android e do computador. No iOS funcionam luz e som.

## Fases da comunicação

O app tem um painel para testar a ligação passo a passo entre dois aparelhos. Cada canal fica verde quando passa:

1. **Sinal:** 1-0-1-0 por 4 s. O receptor confere o ritmo, e o preâmbulo de uma transmissão comum não basta para passar.
2. **Bits e bytes:** 7 bytes fixos (`55 AA 0F F0 41 42 43`) conferidos byte a byte.
3. **Mensagem de teste:** um quadro completo (`TESTE-OK 123`) com checksum.
4. **Conversa:** uma mensagem livre de verdade.

O **Registro da comunicação** mostra cada byte recebido, os quadros descartados e o resultado das fases.

## Protocolo (luz e som)

Funciona como uma porta serial (UART): a linha parada vale 0, e cada byte tem 1 bit de início, 8 bits de dados (o menos significativo primeiro) e 2 bits de parada. Antes de cada transmissão vai um preâmbulo 1010… para o receptor se calibrar.
Quadro: `STX (0x02)` + texto UTF-8 (até 200 bytes) + checksum XOR (em 0x80–0xFF) + `ETX (0x03)`. Um quadro com erro é descartado.

## Limites

- Luz e som são lentos (~5–10 caracteres/s). Servem para mensagens curtas.
- No navegador não dá para usar Bluetooth direto entre celulares, Wi-Fi Direct nem infravermelho. Para isso use o [Briar](https://briarproject.org).
- Os testes automáticos simulam câmera e microfone. O resultado em aparelhos reais depende de luz ambiente, distância, ruído e qualidade do alto-falante.

## Próximos passos

- Correção de erros (Reed-Solomon) para luz e som, em vez de só descartar quadros com erro.
- Repasse entre aparelhos (store-and-forward), para uma mensagem pular de celular em celular.
- App nativo (Tauri) usando o mesmo `nm-core`, para Bluetooth direto entre celulares.
