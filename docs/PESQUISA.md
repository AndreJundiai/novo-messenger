# Pesquisa de produto nº 1 — o que falta nos mensageiros, para quem vive sem sinal

Feita em 18/11/2026 com a skill `whatsapp-product-intelligence`. Foco: o que o Novo Messenger deve priorizar, considerando que ele funciona sem internet.

**Como ler.** Cada afirmação traz o tipo de evidência:
- **[quantitativa]** pesquisa ou dado oficial;
- **[recorrente]** a mesma reclamação em várias fontes;
- **[relato]** reclamação individual ou reportagem com poucos casos;
- **[hipótese]** conclusão minha, ainda não validada com usuários.

As notas de prioridade (seção 6) são **estimativas minhas**, não medição de demanda.

---

## 1. Resumo da pesquisa

1. **Golpes são o maior problema concreto dos mensageiros no Brasil.** 64% das pessoas que sofreram tentativa de fraude foram abordadas por app de mensagens, e as perdas estimadas são de R$ 21,2 bilhões ([quantitativa], GASA 2026, via Mobile Time e CNN Brasil).
2. **Milhões de brasileiros não têm sinal de celular.** As estimativas vão de 10 a 13,7 milhões de pessoas em áreas rurais sem cobertura, e cerca de 66% das localidades rurais analisadas pela Anatel não têm 4G ([quantitativa], PERT 2025; as fontes divergem na métrica).
3. **Os apps sem internet que existem são de nicho e têm limites claros:** alcance curto, dependência de ter gente por perto, só texto, segurança pouco auditada, e nenhum conversa com o outro ([recorrente]).
4. **O WhatsApp está correndo atrás em privacidade.** Os nomes de usuário começaram a ser reservados em junho de 2026, mas o número de telefone continua obrigatório ([quantitativa/oficial], Al Jazeera, 30/06/2026).
5. **Conclusão [hipótese]:** o maior diferencial do Novo Messenger não é só funcionar sem sinal. É ser **à prova do "golpe do número novo" por construção**: um contato só entra pelo QR, ao vivo, e ninguém de fora consegue mandar mensagem.

## 2. Principais reclamações

| Reclamação | Evidência | Fonte (data) |
|---|---|---|
| Golpes por mensagem (falso parente, número novo, Pix) | [quantitativa] 64% das tentativas de fraude por app de mensagens; R$ 21,2 bi perdidos; 34 bi de tentativas entre mar/2025 e fev/2026 | [Mobile Time, 06/08/2026](https://www.mobiletime.com.br/noticias/06/08/2026/golpes-whatsapp-brasil/) · [CNN Brasil](https://www.cnnbrasil.com.br/nacional/brasil/golpes-digitais-brasil-teve-34-bilhoes-de-tentativas-em-2025/) |
| Idosos são a maioria das vítimas | [quantitativa, fonte secundária] 71,7% das vítimas de golpes iniciados por WhatsApp têm mais de 60 anos (dado atribuído à Febraban; não achei o relatório original) | [BV Inspira](https://www.bv.com.br/bv-inspira/seguranca/golpe-do-whatsapp) · [Estado de Minas, 08/2026](http://www.em.com.br/tecnologia/2026/08/7480193-golpes-mais-aplicados-no-brasil-em-2026-como-se-prevenir.html) |
| Notificações que não chegam | [recorrente, fonte secundária] apontada como a principal reclamação numa comparação de 2026; causa frequente é a economia de bateria do Android | [Unstar, 2026](https://unstar.app/blog/messaging-app-reviews-whatsapp-telegram-signal-imessage-2026) |
| Perder anos de conversa ao trocar de celular | [relato] casos citados numa compilação de avaliações | [Unstar, 2026](https://unstar.app/blog/messaging-app-reviews-whatsapp-telegram-signal-imessage-2026) |
| Backup agora ocupa o espaço do Google Drive | [quantitativa/oficial] mudança de política: o backup do Android passou a contar na cota de 15 GB | [TechRadar](https://techradar.com/news/say-goodbye-to-unlimited-whatsapp-backups-in-google-drive) · [Android Central](https://androidcentral.com/whatsapp-android-users-may-soon-lose-unlimited-google-drive-backups) |
| Ser colocado em grupos de golpe sem pedir | [relato] | [Unstar, 2026](https://unstar.app/blog/messaging-app-reviews-whatsapp-telegram-signal-imessage-2026) |
| IA imposta que não dá para tirar (botão do Meta AI) | [recorrente] cobertura em vários veículos em abril de 2025; um eurodeputado questionou a Comissão Europeia | [TechRadar](https://www.techradar.com/computing/websites-apps/whatsapp-users-fume-over-new-meta-ai-button-that-you-cant-remove-heres-what-it-does) · [Geo TV](https://www.geo.tv/amp/601515-whatsapp-users-express-frustration-over-non-removable-meta-ai-feature) |
| App de computador pesado e que desconecta | [recorrente] (Windows) | [Digital Trends](https://www.digitaltrends.com/social-media/whatsapp-for-windows-feels-worse-than-ever-and-im-tired-of-pretending-otherwise/) |

## 3. Funcionalidades mais solicitadas

| Funcionalidade | Situação no WhatsApp | Fonte |
|---|---|---|
| Esconder o número de telefone | Reserva de nome de usuário desde jun/2026, liberação gradual; **o número continua obrigatório** para criar a conta | [Al Jazeera, 30/06/2026](https://www.aljazeera.com/news/2026/6/30/whatsapp-to-let-users-go-by-usernames-not-phone-numbers) · [9to5Google, 08/04/2026](https://9to5google.com/2026/04/08/whatsapp-username-support-rolling-out/) · [BleepingComputer](https://www.bleepingcomputer.com/news/security/whatsapp-rolls-out-usernames-to-help-users-hide-their-phone-number/) |
| Agendar mensagens | Em teste (fev–jun/2026): de 10 min a 2 semanas de antecedência; sem lançamento confirmado | [MacRumors, 24/02/2026](https://www.macrumors.com/2026/02/24/whatsapp-scheduled-messages-coming/) · [TechRepublic](https://www.techrepublic.com/article/news-whatsapp-scheduled-messages-android-beta/) |
| Conversar sem internet | Não existe | — |

Não verificado nesta pesquisa (conhecimento prévio, conferir antes de usar): o Telegram tem mensagens agendadas há anos, e o Signal tem nomes de usuário desde 2024.

## 4. Problemas encontrados nos concorrentes sem internet

| App | Como funciona | Limitações relatadas | Fonte |
|---|---|---|---|
| **Bitchat** | Malha por Bluetooth; usa a rede Nostr quando há internet | Alcance de ~10–100 m por salto; depende de ter gente por perto; só texto; gasta mais bateria; sem auditoria externa nos relatos de 2025; sem verificação de identidade | [Wikipedia](https://en.wikipedia.org/wiki/BitChat) · [The Tech Loft, 01/2026](https://www.thetechloft.com/2026/01/bitchat-app-review-jack-dorsey-bluetooth-mesh-chat.html) · [Bitrue](https://www.bitrue.com/blog/is-bitchat-safe-review) |
| **Briar** | Bluetooth/Wi-Fi e Tor | Só Android; ~2,6 milhões de downloads | [Cybernews](https://cybernews.com/security/iran-sparks-interest-in-bluetooth-based-messengers/) |
| **Bridgefy** | Malha por Bluetooth | Precisa de internet na primeira ativação; segurança contestada no passado | [Wikipedia](https://en.wikipedia.org/wiki/Bridgefy) · [Secumeet, 2026](https://secumeet.com/reviews/offline-messaging-apps) |
| **Meshtastic** | Rádio LoRa (quilômetros) | Exige comprar um rádio; app técnico; há comunidade no Brasil (915 MHz) e pesquisa da UFMS sobre uso rural | [UFMS, 2026](https://repositorio.ufms.br/retrieve/6f65441f-0a8a-4594-a388-30269c3a0a57/37065.pdf) · [awesome-meshtastic](https://github.com/ShakataGaNai/awesome-meshtastic/) |
| Todos | — | **Não conversam entre si**: as duas pessoas precisam ter o mesmo app | [Secumeet, 2026](https://secumeet.com/reviews/offline-messaging-apps) |

## 5. Oportunidades de inovação

1. **À prova de golpe por construção [hipótese forte].** No WhatsApp, quem tem seu número fala com você, e o "mãe, troquei de número" funciona porque qualquer número novo pode se apresentar como alguém. No Novo Messenger, um contato só entra pelo QR, presencialmente, e a criptografia já impede quem não é contato de entregar mensagem (isso já está implementado no `nm-chat`). Falta deixar isso **visível** e cobrir os casos que sobram (abaixo).
2. **Uma só rede para todos os meios.** Celular, rádio LoRa e "pombo-correio" juntos, e falando com rádios Meshtastic que já existem. Os concorrentes não conversam entre si; ser compatível com o Meshtastic é diferencial.
3. **Backup que não depende de nuvem.** Arquivo cifrado com senha, passado de celular para celular sem internet. Responde à reclamação do Google Drive e da perda de histórico.
4. **IA no próprio aparelho, opcional e desligada por padrão.** Responde à rejeição do Meta AI imposto. Primeiro uso útil: avisar sobre pedido de dinheiro.
5. **Notificação que chega.** O serviço em primeiro plano já mantém o app acordado; falta guiar a pessoa a liberar a economia de bateria.

## 6. Ranking de prioridades

Notas de 0 a 10 **estimadas por mim** a partir das evidências acima. Fórmula da skill: `F×0,25 + I×0,25 + D×0,20 + Dif×0,15 + V×0,15`.

| # | Oportunidade | Freq. | Impacto | Demanda | Diferencial | Viabilidade | **Prioridade** |
|---|---|---|---|---|---|---|---|
| 1 | Escudo contra golpes | 9 | 9 | 8 | 8 | 8 | **8,50 · muito alta** |
| 2 | Entrega sem sinal (malha + pombo-correio) — já é a fase 2 do plano | 7 | 9 | 6 | 9 | 6 | **7,45 · alta** |
| 3 | Backup cifrado sem nuvem | 6 | 8 | 6 | 6 | 9 | **6,95 · alta** |
| 4 | Grupos só com consentimento | 7 | 6 | 6 | 5 | 9 | **6,55 · alta** |
| 5 | Notificação confiável (guia de bateria) | 7 | 7 | 7 | 4 | 7 | **6,55 · alta** |
| 6 | Áudio curto pelo rádio (Codec2) — "brasileiro usa muito áudio" é hipótese a validar | 6 | 7 | 5 | 8 | 5 | **6,20 · alta** |
| 7 | IA local opcional (resumo, alerta) | 5 | 6 | 4 | 7 | 4 | **5,20 · média** |
| 8 | Agendar mensagens (o WhatsApp está lançando) | 4 | 4 | 6 | 2 | 8 | **4,70 · média** |

## 7. Proposta de funcionalidades

### 7.1 Escudo contra golpes (prioridade 1)

**Problema.** Golpistas se passam por parentes ("troquei de número") e pedem Pix. As vítimas são sobretudo idosos.

**Solução.** Quatro proteções, todas no aparelho, sem enviar nada para servidor nenhum:
1. **Contato só ao vivo:** a única forma de adicionar alguém é ler o QR dele. Isso já existe; falta explicar isso na hora de adicionar.
2. **Selo "conferido pessoalmente":** contato adicionado por QR ganha um selo. Se um dia houver outra forma de adicionar (convite por link na fase 4), ela não ganha o selo.
3. **Mesmo nome, pessoa diferente:** se alguém novo tiver o mesmo nome (ou parecido) de um contato que você já tem, o app avisa: *"Você já tem uma Mãe. Essa é outra pessoa, com outro código de segurança."*
4. **Alerta de dinheiro:** quando chega uma mensagem que fala em Pix, transferência, depósito, boleto ou código, aparece uma faixa acima dela: *"Pedido de dinheiro. Antes de pagar, confirme pessoalmente ou por ligação."* A análise é por palavras, no próprio aparelho. Pode ser desligada.

**Experiência do usuário.** Ler o QR → tela de confirmação mostrando nome, código de segurança e "Adicionado pessoalmente ✓" → conversa. Num pedido de dinheiro, a faixa amarela aparece na bolha, com o botão "Entendi".

**Regras de negócio.**
- O alerta de dinheiro aparece em mensagens recebidas, nunca nas enviadas.
- Não bloqueia nada, só avisa: a pessoa decide.
- "Parecido" = mesmo nome ignorando acentos, maiúsculas e espaços, ou distância de edição ≤ 1.

**Requisitos técnicos.**
- `nm-chat`: campo `added_in_person` no contato (persistido); função `similar_contacts(nome)`; detector `money_request(texto) -> bool` com lista de termos em português, sem acento.
- `nm-ffi`: expor `ContactInfo.addedInPerson`, `MessageInfo.moneyAlert` e `similarContacts`.
- Android: faixa na bolha, tela de confirmação após o QR, opção nas configurações.
- Sem banco novo: o formato salvo ganha uma versão 2, e o app migra sozinho o arquivo antigo.

**Segurança e privacidade.** Nada sai do aparelho; o detector não registra o que leu. A criptografia de ponta a ponta continua igual.

**Critérios de aceitação.**
- Uma mensagem "me manda um pix de 500" mostra o alerta; "comprei pão" não mostra.
- Adicionar um segundo "Mãe" mostra o aviso de nome parecido.
- O formato salvo antigo continua abrindo depois da atualização.

**Testes.**
- Unitários do detector: lista de frases positivas e negativas, com e sem acento, maiúsculas e erros de digitação comuns.
- Unitários de nome parecido.
- Teste de migração do formato salvo.
- Teste de interface: a faixa aparece e some ao tocar em "Entendi".

### 7.2 Backup cifrado sem nuvem (prioridade 3)

**Problema.** Perder conversas ao trocar de celular; o backup do Google Drive agora ocupa espaço da cota.

**Solução.** "Exportar backup" gera um arquivo cifrado com uma senha escolhida pela pessoa. Ele pode ir para outro celular pelo próprio app (conexão direta, sem internet), por cartão de memória ou por cabo. "Restaurar" pede a senha.

**Requisitos técnicos.** Senha → chave com Argon2id (crate `argon2`) → ChaCha20-Poly1305 sobre o `Chat::save()` que já existe. O Android usa o seletor de arquivos do sistema (Storage Access Framework), sem pedir permissão de armazenamento.

**Segurança.** Sem a senha o arquivo não abre, e senha esquecida não tem recuperação. Isso precisa ser dito na tela antes de exportar.

**Critérios de aceitação.** Exportar num aparelho e restaurar em outro devolve as mesmas conversas e contatos, e as mensagens pendentes continuam sendo reenviadas. Senha errada mostra "senha incorreta" e não estraga nada.

### 7.3 Grupos só com consentimento (prioridade 4)

Entrar num grupo exige aceitar um convite, e ninguém é colocado sem pedir. Fica para quando os grupos existirem (ainda não existem no app).

## 8. Requisitos técnicos (resumo)

| Item | Onde | Dependência nova |
|---|---|---|
| Detector de pedido de dinheiro | `nm-chat` | nenhuma |
| Nome parecido | `nm-chat` | nenhuma |
| Selo "adicionado pessoalmente" + migração do formato salvo | `nm-chat`, `nm-ffi` | nenhuma |
| Backup cifrado | `nm-chat` | `argon2` (crates.io) |
| Telas | `android/` | nenhuma |

## 9. Sugestão de interface

- **Depois de ler o QR:** cartão com avatar, nome, "Adicionado pessoalmente ✓" e código de segurança. Botões "Conversar" e "Renomear".
- **Bolha com alerta:** faixa amarela no topo da bolha recebida: "⚠ Pedido de dinheiro. Confirme pessoalmente antes de pagar." Botão "Entendi".
- **Configurações → Segurança:** "Avisar sobre pedidos de dinheiro" (ligado por padrão) e "Exportar backup cifrado".
- **Acessibilidade:** a faixa tem descrição para leitor de tela, e o contraste foi pensado para quem enxerga pouco (público mais velho).

## 10. Plano de implementação

| Ordem | Entrega | Cabe em |
|---|---|---|
| 1 | Escudo contra golpes (7.1) | fase 1.1 |
| 2 | Backup cifrado sem nuvem (7.2) | fase 1.1 |
| 3 | Guia para liberar a economia de bateria (notificações) | fase 1.1 |
| 4 | Malha de celulares + pombo-correio | fase 2 (já no PLANO.md) |
| 5 | Compatibilidade com rádios Meshtastic + áudio curto | fase 3 |
| 6 | Grupos com consentimento | depois dos grupos |

## Próximas pesquisas sugeridas

- Ler as avaliações negativas recentes do WhatsApp, Bitchat e Briar na Play Store brasileira (não acessadas nesta rodada).
- Conferir no relatório original da Febraban o dado de 71,7% de vítimas acima de 60 anos.
- Entrevistar 5 a 10 pessoas de zona rural sem sinal para validar as hipóteses do áudio e do pombo-correio.
