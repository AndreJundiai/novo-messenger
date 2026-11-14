# Novo Messenger

Mensagens sem internet, sem sinal de operadora e sem créditos. Um app web (PWA) que manda texto por três meios ao mesmo tempo:

| Canal | Como funciona | Alcance | Precisa de algo extra? |
|---|---|---|---|
| **Luz** | A tela (ou a lanterna no Android) pisca os bits; o outro celular lê com a câmera | 10–30 cm, de frente | Não |
| **Som** | Dois tons (FSK) pelo alto-falante; o outro celular escuta pelo microfone. Há um modo quase inaudível (17,5/18,5 kHz) | 1–2 m | Não |
| **LoRa** | Conecta por Bluetooth a um rádio [Meshtastic](https://meshtastic.org) e manda pelo rádio | vários km | Rádio LoRa (~R$ 100–200) |

Marque mais de um canal: a mensagem sai por todos e o receptor junta as cópias repetidas.

## Como usar

1. Publique no GitHub Pages (Settings → Pages → branch `main`, pasta `/`). A câmera, o microfone e o Bluetooth exigem HTTPS.
2. Abra o link **uma vez com internet** e use "Adicionar à tela inicial". Depois disso o app abre sem rede (service worker).
3. No celular que recebe, toque em **Ouvir pela câmera** e/ou **Ouvir pelo microfone**.
4. No celular que envia, escreva e toque em **Enviar**.

Os dois aparelhos precisam usar o mesmo valor de "Bit (ms)" e as mesmas frequências.

## Protocolo (luz e som)

Funciona como uma porta serial (UART): a linha parada vale 0, e cada byte tem 1 bit de início, 8 bits de dados (o menos significativo primeiro) e 2 bits de parada.
Quadro: `STX (0x02)` + texto UTF-8 + checksum XOR (em 0x80–0xFF) + `ETX (0x03)`. Um quadro com erro é descartado.

## Limites

- No navegador não dá para usar Bluetooth direto entre celulares, Wi-Fi Direct nem infravermelho. Para isso use o [Briar](https://briarproject.org), um app nativo.
- Luz e som são lentos (~5–10 caracteres/s). Servem para mensagens curtas.

## Testes

```
node test.js
```

Testa o protocolo com amostragem irregular (simula câmera e microfone), a rejeição por checksum e o protobuf do Meshtastic.
