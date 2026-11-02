# Comece aqui
## Seu primeiro documento
?? primeiro início começar parágrafo texto
No Typst, você escreve o texto como ele é. Uma linha em branco começa um novo parágrafo. À esquerda fica o código; à direita, as páginas prontas.

Os exemplos deste painel são vivos: edite o código abaixo e veja o resultado mudar na hora.
```example
Olá, mundo!

Este é o segundo parágrafo.
```
> Tente: troque "mundo" pelo seu nome e acrescente um terceiro parágrafo.
## Negrito, itálico e sublinhado
?? negrito itálico sublinhado destaque ênfase formatação
Use `*asteriscos*` para negrito e `_sublinhados_` para itálico. Para o restante, chame uma função com `#`, como `#underline[...]`.
```example
Isto é *importante*, e isto é _delicado_.

#underline[Sublinhado] e #strike[riscado].

#highlight[Marca-texto] também funciona.
```
> Tente: deixe uma palavra em negrito e itálico ao mesmo tempo.
## Títulos
?? título seção subseção cabeçalho heading numeração
Comece a linha com `=`. Cada `=` a mais desce um nível.
```example
= Capítulo
== Seção
=== Subseção
Texto da subseção.
```
Para numerar os títulos, use uma regra no começo do documento:
```example
#set heading(numbering: "1.1")
= Introdução
== Objetivos
= Métodos
```
> Tente: mude a numeração para "I.a".
## Listas
?? lista marcadores numerada itens tópicos
Use `-` para marcadores e `+` para listas numeradas. Recue com espaços para criar subitens.
```example
- Primeiro
- Segundo
  - Um detalhe

+ Passo um
+ Passo dois
```
> Tente: crie uma lista de compras com um subitem.
# Layout
## Página e margens
?? página margem papel a4 a5 tamanho paisagem numeração de páginas
A regra `#set page` define papel, margens e numeração. Vale do ponto onde aparece em diante. No Pearl, você também pode arrastar as margens na régua.
```text
#set page(paper: "a4", margin: 2.5cm, numbering: "1")
```
```example
#set page(width: 8cm, height: 4cm, margin: 1cm, fill: luma(240))
Uma página pequena, só para ver o efeito.
```
> Tente: aumente a margem para 2cm e veja o texto se ajustar.
## Fonte e tamanho
?? fonte tamanho letra cor idioma texto font size
A regra `#set text` muda como o texto aparece daí em diante. Declare também o idioma: ele define a hifenização e palavras como "Figura".
```example
#set text(size: 14pt, fill: blue, lang: "pt")
Texto azul e maior.

#text(size: 9pt, fill: gray)[Só este trecho é menor.]
```
> Tente: troque a cor por `red` e o tamanho por 18pt.
## Alinhamento e espaçamento
?? alinhar centralizar justificar espaço entrelinhas parágrafo recuo
`#align` posiciona um bloco, e `#set par` controla os parágrafos. `#v` e `#h` criam espaços.
```example
#align(center)[Centralizado]
#v(8pt)
#align(right)[À direita]

#set par(justify: true, first-line-indent: 1em)
Este parágrafo fica justificado e com recuo na primeira linha, como em muitos trabalhos acadêmicos.
```
> Tente: mude `center` para `left`.
## Colunas e grades
?? colunas grid grade lado a lado
`#columns` divide o texto em colunas. `#grid` coloca coisas lado a lado sem bordas.
```example
#grid(
  columns: (1fr, 1fr),
  gutter: 8pt,
  rect(width: 100%)[Esquerda],
  rect(width: 100%)[Direita],
)
```
> Tente: acrescente uma terceira coluna.
# Conteúdo
## Imagens e figuras
?? imagem figura foto legenda picture colar
No Pearl, a forma mais fácil é colar a imagem com Ctrl+V ou usar o botão da aba Inserir: o arquivo vai para a pasta do documento e o código entra sozinho.
```text
#figure(
  image("images/foto.png", width: 60%),
  caption: [Minha legenda],
)
```
`#figure` funciona com qualquer conteúdo, e numera sozinha:
```example
#figure(
  rect(width: 3cm, height: 1.5cm, fill: aqua),
  caption: [Um retângulo],
)
```
> Tente: mude a largura do retângulo e a legenda.
## Tabelas
?? tabela linhas colunas células table cabeçalho
Diga quantas colunas a tabela tem e liste as células em ordem.
```example
#table(
  columns: 2,
  [*Nome*], [*Nota*],
  [Ana], [9,5],
  [Beto], [8,0],
)
```
> Tente: acrescente a coluna "Faltas".
## Links e notas de rodapé
?? link endereço url nota rodapé footnote
Endereços viram links sozinhos. `#link` dá outro texto a eles, e `#footnote` cria uma nota no pé da página.
```example
Veja https://typst.app ou o #link("https://typst.app")[site do Typst].

Uma afirmação.#footnote[A explicação fica aqui.]
```
> Tente: acrescente uma segunda nota.
# Matemática
## Fórmulas
?? matemática fórmula equação math cifrão inline bloco
Coloque a fórmula entre `$`. Sem espaços nas pontas, ela fica na linha; com espaços, vira um bloco centralizado. O painel Fórmulas, na barra à direita, traz mais de 90 fórmulas prontas de álgebra, cálculo, estatística e outras áreas, e o botão Fórmulas prontas fica na aba Matemática.
```example
A área é $A = pi r^2$ neste caso.

$ E = m c^2 $
```
> Tente: escreva $a^2 + b^2 = c^2$ em bloco.
## Frações, raízes e símbolos
?? fração raiz somatório integral letras gregas símbolos índice expoente
Uma barra cria fração. Símbolos têm nomes por extenso, como `alpha` e `sum`. Veja a lista completa no painel Fórmulas, em Símbolos.
```example
$ x = (-b plus.minus sqrt(b^2 - 4 a c)) / (2 a) $

$ sum_(i=1)^n i = (n(n+1)) / 2 $

$ alpha + beta >= gamma $
```
> Tente: escreva a integral de x de 0 a 1 com `integral_0^1 x dif x`.
## Matrizes e equações numeradas
?? matriz numerada equação alinhada cases sistema
`mat` e `cases` montam estruturas. `&` alinha as linhas, e a regra de `math.equation` numera as equações.
```example
#set math.equation(numbering: "(1)")
$ A = mat(1, 2; 3, 4) $

$ f(x) &= x^2 + 2x \
       &= x(x + 2) $
```
> Tente: mude a matriz para 3 por 3.
# Referências
## Rótulos e referências cruzadas
?? rótulo label referência cruzada ref arroba figura seção
Dê um nome com `<nome>` e aponte para ele com `@nome`. A numeração se atualiza sozinha.
```example
#set heading(numbering: "1.")
= Métodos <metodos>
Como descrito na @metodos e na @fig-a.

#figure(rect(width: 1cm, height: 1cm), caption: [Um quadrado]) <fig-a>
```
> Tente: crie outro título com rótulo e cite-o.
## Bibliografia e citações
?? bibliografia citação cite bib doi abnt apa referências
Abra o painel Referências: importe um `.bib` ou cole um DOI, e clique para citar. O Pearl adiciona a bibliografia quando falta. Em código, fica assim:
```text
Como mostrou @lecun2015.

#bibliography("refs.bib", style: "associacao-brasileira-de-normas-tecnicas")
```
O botão **Verificar na internet** do painel confere se cada referência é encontrada em bases públicas.
## Sumário e listas
?? sumário índice outline lista de figuras tabelas
`#outline` monta o sumário a partir dos títulos, e pode listar também figuras e tabelas.
```example
#outline()

= Introdução
= Conclusão
```
```text
#outline(title: [Figuras], target: figure.where(kind: image))
```
> Tente: mude o título do sumário com `title: [Índice]`.
# Programação
## Variáveis
?? variável let valor expressão cálculo
`#let` guarda um valor, e `#` o usa no texto. O Typst também faz contas.
```example
#let nome = "Pearl"
#let total = 3 * 7

O #nome calculou #total.
```
> Tente: crie a variável `ano` e mostre o resultado de `ano + 1`.
## Funções
?? função def parâmetro argumento reutilizar
`#let` também cria funções, úteis para repetir um formato.
```example
#let aviso(corpo) = rect(fill: yellow, inset: 6pt)[*Atenção:* #corpo]

#aviso[Salve antes de sair.]
#aviso[Confira as referências.]
```
> Tente: mude a cor do aviso.
## Regras set e show
?? set show regra estilo modelo template transformar
`#set` define valores padrão, e `#show` transforma o que aparece. Assim, o estilo fica em um só lugar.
```example
#show heading: set text(fill: maroon)
#show "Pearl": name => strong(name)

= Um título colorido
O Pearl aparece em negrito aqui.
```
> Tente: mude a cor dos títulos.
## Condições e repetições
?? if else for while laço condição repetição
`if` escolhe, e `for` repete. Dentro de `#[...]`, volta-se a escrever texto.
```example
#let nota = 8

#if nota >= 7 [Aprovado] else [Reprovado]

#for i in range(1, 4) [
  - Item #i
]
```
> Tente: mude a nota para 5.
# O Pearl
## Digitar nas páginas
?? pearl digitar página editar clicar cursor wysiwyg
Clique em qualquer texto da página: o cursor aparece ali, e o que você digita entra no código. Isso funciona também na exibição só de páginas. Um clique em branco leva ao texto mais próximo. O código continua sendo a única fonte: desfazer, sessões e histórico seguem iguais.
## Régua e margens
?? pearl régua margens arrastar
Arraste o limite entre a área cinza e a clara da régua para mudar a margem. Ao soltar, o Pearl atualiza a regra `#set page` no código, e um único Ctrl+Z desfaz tudo.
## Sessões e Claude
?? pearl sessão colaborar tempo real claude ia link
Abra uma sessão pelo ícone de compartilhar, no alto da janela, e envie o link: quem entra edita o mesmo documento, de forma criptografada. O painel Claude conversa sobre o documento e pode entrar na sessão como mais uma pessoa, usando a sua chave de API.
## Versões e recuperação
?? pearl histórico versão recuperar autosave salvar
O painel Versões guarda o documento ao salvar: veja o que mudou, olhe as páginas antigas e restaure. Se o programa fechar de repente, o texto não salvo é oferecido quando ele abrir de novo.
## Atalhos
?? pearl atalhos teclado f1 ajuda
F1 abre esta documentação e busca a palavra no cursor. Ctrl+F busca no documento, Ctrl+S salva, Ctrl+E exporta o PDF, Ctrl+Espaço pede sugestões, e Ctrl+B e Ctrl+I aplicam negrito e itálico.
## Figuras nas páginas
?? pearl figura imagem redimensionar mover arrastar apagar página
Clique em uma imagem na página para selecioná-la. Arraste o quadradinho do canto para mudar o tamanho: o Pearl escreve o `width:` no código. Arraste a própria imagem para movê-la até outro parágrafo. A tecla Delete a tira do texto, sem apagar o arquivo, e Esc desfaz a seleção. Cada ação é desfeita com um único Ctrl+Z.
## Timbrado e marca d'água
?? timbrado marca d'água rascunho logotipo cabeçalho rodapé papel
Na aba Layout da Página, o botão Timbrado e marca d'água abre uma janela com prévia ao vivo: nome, endereço, logotipo e rodapé de um lado, e texto ou imagem de fundo do outro, a imagem com sua opacidade. Ao aplicar, o Pearl escreve uma regra `#set page` no início do documento e a reabre com os mesmos valores. Sem o botão, seria assim:
```example
#set page(width: 10cm, height: 6cm, background: rotate(-45deg, text(30pt, fill: luma(225), "RASCUNHO")))
Um texto sobre a marca d'água.
```
> Tente: troque "RASCUNHO" por "CÓPIA" e mude o ângulo.
## Vigia de tipografia
?? tipografia viúva órfã título margem tabela figura problemas
O painel Problemas também aponta o que fica ruim nas páginas prontas: a primeira ou a última linha de um parágrafo sozinha em uma página, um título sozinho no fim da página, tabelas e texto além da margem, e figuras a duas ou mais páginas da primeira menção. Com o painel aberto, cada item fica marcado na página, e um clique leva ao trecho. Ignorar cala o aviso até o trecho mudar. Só o texto corrido deste arquivo é lido, e páginas em colunas não são verificadas quanto a viúvas e órfãs.
## Comparar versões nas páginas
?? pearl comparar versão diferença revisão antes depois histórico
No painel Versões, escolha uma versão e clique em Comparar nas páginas. As páginas de antes e de agora ficam lado a lado: verde para o que entrou, vermelho para o que saiu e azul para o que mudou de lugar. Os botões Anterior e Próxima percorrem as mudanças, e nada no documento é alterado.
