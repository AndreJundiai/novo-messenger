# Plano de implementação

Objetivo: um mensageiro com a simplicidade do WhatsApp que funciona **sem internet, sem sinal de operadora e sem créditos**, e que alcança **longas distâncias**.

## O limite físico que guia o plano

O rádio do celular (Bluetooth, Wi-Fi) alcança algumas centenas de metros. Nenhum software muda isso. Longa distância sem internet só acontece de três formas, e o plano junta as três numa rede só:

| Distância | Como a mensagem viaja | Custo |
|---|---|---|
| até ~200 m | Celular com celular (Bluetooth + Wi-Fi direto). Luz, som e QR como reserva | grátis |
| 5–20 km por salto | Rádio LoRa: rádio de bolso pareado ao celular + repetidores solares em pontos altos | ~R$150 por rádio, sem mensalidade |
| qualquer distância, com atraso | **Pombo-correio digital** (rede tolerante a atrasos): a mensagem cifrada viaja guardada nos celulares de quem se desloca e é entregue quando essa pessoa passa perto do destino | grátis |
| opcional | Se qualquer aparelho pegar internet (Wi-Fi público, satélite), ele sincroniza o que estiver pendente | grátis |

A mensagem escolhe sozinha o caminho que existir. Quem usa só vê ✓ / ✓✓ / ✓✓ azul.

## Arquitetura

```
 App Android (Kotlin + Jetpack Compose)        App iPhone (Swift + SwiftUI) — fase 4
            │  UniFFI                                      │
            └──────────────┬───────────────────────────────┘
                     nm-ffi (Rust)
                           │
   nm-core (Rust, já existe): quadros, Reed-Solomon, QR, luz, som, Meshtastic
   nm-chat (Rust, fase 1):   identidade, criptografia, conversas, confirmações
   nm-dtn  (Rust, fase 2):   guardar e repassar, roteamento entre canais
                           │
 Transportes: Nearby (BT + Wi-Fi) · Wi-Fi Aware · LoRa (rádio pareado) · luz/som/QR · internet
```

## Fases

| Fase | Entrega | Pronto quando |
|---|---|---|
| **0 ✅** | Núcleo Rust: luz, som, QR, LoRa, correção de erros, histórico CRDT, ponte | feito |
| **1** | App Android com a cara do WhatsApp: conversas, ✓/✓✓/azul, contato por QR (sem número de telefone), criptografia de ponta a ponta, conexão direta celular a celular | duas pessoas conversam a 20 m com o app em segundo plano |
| **2** | Malha de celulares e pombo-correio: cada celular guarda e repassa mensagens cifradas dos outros, sem conseguir lê-las | mensagem atravessa 3 celulares e chega mesmo se o destinatário aparecer 2 h depois |
| **3** | Longa distância: rádio LoRa de bolso + repetidor solar com firmware próprio; áudio curto por rádio (Codec2, ~2 KB para 10 s) | 20 km com 2 repetidores; 50 km+ com visada livre |
| **4** | Sincronização pela internet quando aparecer; versão iPhone | mensagem atravessa uma cidade por meios mistos |
| **5** | Piloto real numa comunidade (10–20 repetidores), medindo entrega e atraso | mais de 95% das mensagens entregues |

## Fase 1 em detalhe

**Identidade.** Cada pessoa é uma chave X25519 gerada no aparelho. Não há número de telefone nem servidor. Adicionar contato = ler o QR do outro (`NMC1:` + chave + nome).

**Criptografia.** Cada mensagem é cifrada para o destinatário com ChaCha20-Poly1305. A chave vem de dois acordos Diffie-Hellman: uma chave efêmera (sigilo se a chave longa vazar depois) e as chaves fixas das duas pessoas (prova de quem enviou). Quem repassa a mensagem não consegue ler nem alterar.

**Envelope** (cabe num quadro de luz/som e num pacote LoRa):
`[versão] [dica do destinatário: 4 bytes] [chave efêmera: 32] [nonce: 12] [conteúdo cifrado + tag: 16]`.
O conteúdo cifrado leva o tipo (texto, entregue, lido), o id da mensagem, a chave de quem enviou e o texto.

**Confirmações.** ✓ = salva e saiu do aparelho · ✓✓ = o destinatário recebeu (ele devolve um "entregue" cifrado) · ✓✓ azul = leu.

**Transporte da fase 1.** Nearby Connections do Android (Bluetooth + Wi-Fi direto, sem internet), mantido por um serviço em primeiro plano para receber com a tela desligada. Ao se conectarem, os dois aparelhos trocam as mensagens pendentes um para o outro.

**Compilação.** O núcleo Rust é compilado para Android (arm64, armv7, x86_64) com o NDK, e as ligações Kotlin são geradas pelo UniFFI. O GitHub Actions monta o APK e o deixa para download.

## Limites honestos

- Foto e vídeo não passam bem por LoRa: texto, áudio curto e miniatura sim. Por Wi-Fi direto passam normal.
- Longa distância precisa de repetidores físicos ou de pessoas circulando.
- LoRa usa faixa de uso livre (915 MHz) com equipamento homologado pela Anatel. Rádio HF (milhares de km) exige licença de radioamador e fica como ponte opcional.
- O iPhone restringe rádio em segundo plano; Android vem primeiro.
