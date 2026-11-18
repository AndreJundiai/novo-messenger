# Novo Messenger

Mensagens sem internet, sem sinal de operadora e sem créditos. O mesmo núcleo em **Rust** roda no navegador (WebAssembly) e no terminal (binário nativo), e manda texto por quatro meios ao mesmo tempo:

| Canal | Como funciona | Alcance | Precisa de algo extra? |
|---|---|---|---|
| **Luz** | A tela (ou a lanterna no Android) pisca os bits; o outro aparelho lê com a câmera | 10–30 cm, de frente | Não |
| **Som** | Dois tons (FSK) pelo alto-falante; o outro aparelho escuta pelo microfone. Há um modo quase inaudível (17,5/18,5 kHz) | 1–2 m | Não |
| **LoRa** | Conecta por Bluetooth a um rádio [Meshtastic](https://meshtastic.org) e manda pelo rádio | vários km | Rádio LoRa (~R$ 100–200) |
| **QR Code** | A tela mostra a mensagem inteira num QR; o outro aparelho lê com a câmera | até ~1 m | Não |

Marque mais de um canal e a mensagem sai por todos. O receptor junta as cópias repetidas.

Além disso:

- **Correção de erros Reed-Solomon** em luz, som e QR: bytes trocados no caminho são consertados (até 1 byte errado a cada 8, aproximadamente).
- **Histórico local-first (CRDT):** a conversa fica salva no aparelho e pode ser exportada num arquivo `.nmlog`. Importar junta as conversas sem duplicar nada, em qualquer ordem, então dá para sincronizar por Bluetooth ou cabo, sem servidor.
- **Modo ponte:** um aparelho repassa para os outros canais marcados o que chega por um canal. Por exemplo: ouve por som e manda pelo rádio LoRa, alcançando quilômetros. Cada mensagem tem um limite de repasses (TTL), então não fica em ciclo.

## App Android

A pasta `android/` tem o app nativo, com a cara de um mensageiro moderno:

- **Com e sem internet:** com conexão, as mensagens vão por relays públicos do [Nostr](https://nostr.com) (sem servidor próprio); sem conexão, vão direto de celular em celular por Bluetooth e Wi-Fi. O app escolhe sozinho e descarta cópias.
- **Do WhatsApp e do Telegram:** ✓ / ✓✓ / ✓✓ azul, responder citando, reagir com emoji, editar, apagar para todos, apagar para mim e busca nas conversas.
- **Escudo contra golpes:** contato só entra pelo QR, pessoalmente (sem número de telefone); aviso de nome parecido ("troquei de número"); alerta quando uma mensagem pede Pix, transferência ou código. Tudo analisado no próprio aparelho.
- **Agenda do celular:** mostra quais contatos têm o app (para quem ativar o diretório opcional, que publica só um resumo embaralhado do número). Adicionar alguém pelo número manda um **SMS de confirmação**: só o dono do chip consegue confirmar, então quem cadastrar o número de outra pessoa não se passa por ela. Para quem não tem o app: **convite por SMS** ou **SMS comum** (sem cifra, pode gastar créditos).
- **Backup cifrado com senha, sem nuvem.**
- Criptografia de ponta a ponta em todos os caminhos; os relays da internet só veem dados cifrados.

O porquê de cada escolha está em [docs/PESQUISA.md](docs/PESQUISA.md).

- **Baixar o APK:** aba **Actions** → "App Android" → última execução → artefato `novo-messenger-apk`.
- **Compilar na sua máquina:** precisa do Android SDK + NDK, Rust e `cargo install cargo-ndk`. Depois `./android/build-rust.sh` e `cd android && ./gradlew assembleDebug`.

O plano completo, com as próximas fases (malha de celulares, longa distância por LoRa, iPhone), está em [PLANO.md](PLANO.md).

## Arquitetura

```
             ┌──────────────────────── nm-core (Rust, sem dependências) ─────────────────────────┐
 câmera ───► │ light: limiar adaptativo (filas monotônicas, O(1))  ┐                              │
             │                                                      ├► uart ► frame ► session ──► eventos
 microfone ► │ fsk: Goertzel + janela de Hann, tempo pelo relógio   ┘   bits   STX/CRC   fases 1–4    │
             │      de amostras                                                                   │
             │ fsk::synth: áudio com fase contínua   ◄── tx_bits (toggle · bytes de teste · texto)  │
             │ rs: Reed-Solomon GF(2⁸) · message: id + relógio de Lamport + TTL                   │
             │ store: histórico CRDT (G-Set) · qr: gerador de QR Code · mesh: protobuf Meshtastic │
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
  src/frame.rs             quadro com sincronismo, tamanho e paridade Reed-Solomon
  src/rs.rs                Reed-Solomon (codificação e correção)
  src/message.rs           mensagem: id, relógio de Lamport, TTL, texto
  src/store.rs             histórico CRDT, exportar/importar, salvar
  src/qr.rs                gerador de QR Code (nível M, versões 1–40)
  src/b64.rs               Base64 (texto do QR e histórico salvo)
  src/uart.rs              bits ↔ bytes; detector da fase 1
  src/light.rs             brilho da câmera → 0/1
  src/fsk.rs               áudio → 0/1 (Goertzel) e bits → áudio
  src/session.rs           cadeia completa de recepção e fases de teste
  src/mesh.rs              mensagens de texto do Meshtastic
crates/nm-wasm/            ponte WebAssembly (C ABI)
crates/nm-cli/             programa de terminal `nm`
crates/nm-chat/            conversas: identidade, criptografia, confirmações ✓✓, escudo, backup, Nostr
crates/nm-ffi/             ligação com Kotlin/Swift (UniFFI)
android/                   app Android (Kotlin + Jetpack Compose)
web/                       app (index.html, app.js, nm.js, mic-worklet.js, sw.js)
web/vendor/jsQR.js         leitor de QR para aparelhos sem leitor nativo (Apache-2.0)
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
nm qr "Corte às 15h"                     # QR Code no terminal, para o celular ler (--claro em terminal de fundo claro)

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
Quadro: `02 D5` (sincronismo) + tamanho `L` + `!L` + `L` bytes de dados + paridade Reed-Solomon (¼ dos dados, entre 8 e 48 bytes). A paridade corrige metade do seu tamanho em bytes errados, em qualquer posição. Acima disso o quadro é descartado.

Dados: `[versão 2 | TTL] [id u32] [lamport u16] [texto UTF-8, até 200 bytes]`. O QR Code leva o mesmo quadro em Base64, com o prefixo `NM2:`. O rádio LoRa leva texto puro, para conversar com outros apps Meshtastic.

## Limites

- Luz e som são lentos (~5–10 caracteres/s). Servem para mensagens curtas.
- No navegador não dá para usar Bluetooth direto entre celulares, Wi-Fi Direct nem infravermelho. Para isso use o [Briar](https://briarproject.org).
- Os testes automáticos simulam câmera e microfone. O resultado em aparelhos reais depende de luz ambiente, distância, ruído e qualidade do alto-falante.

## Plano

| Etapa | Tecnologia | Situação |
|---|---|---|
| 1 | Protocolo de luz e som, fases de teste | ✅ pronto |
| 2 | Núcleo em Rust → WebAssembly + terminal | ✅ pronto |
| A | Correção de erros Reed-Solomon | ✅ pronto |
| B | Histórico local-first (CRDT), exportar/importar, modo ponte | ✅ pronto |
| C | Canal QR Code (gerador em Rust, leitura nativa ou jsQR) | ✅ pronto |
| D | App nativo (Tauri 2) com o mesmo `nm-core`, para Bluetooth direto entre celulares (BLE) e Wi-Fi Direct | a fazer |
| E | QR animado (várias imagens em sequência) para textos longos e arquivos | a fazer |
| F | Sincronização automática do histórico entre aparelhos próximos (troca de resumos por som/QR) | a fazer |

O que falta depende de testes em aparelhos reais (D precisa de Android/iOS para compilar e testar o Bluetooth).
