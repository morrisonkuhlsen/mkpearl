//! The document a new window starts with.

pub const SAMPLE: &str = r#"#set document(title: "Bem-vindo ao Pearl")
#set text(lang: "pt", size: 11pt)
#set par(justify: true)
#set heading(numbering: "1.1")

#align(center, text(20pt, weight: "bold")[Bem-vindo ao Pearl])

= Introdução
Escreva à esquerda e veja a página à direita. O Typst compila *de forma
incremental*, então só o que mudou é refeito. Clique em qualquer parte da
página para ir até o _trecho do código_ correspondente.

= Matemática
A soma dos $n$ primeiros inteiros é dada por:

$ sum_(k=1)^n k = (n(n+1)) / 2 $

= Tabelas
#figure(
  table(
    columns: 3,
    table.header([*Motor*], [*Linguagem*], [*Incremental*]),
    [Typst], [Rust], [Sim],
    [pdfTeX], [Pascal/C], [Não],
  ),
  caption: [Comparação entre motores de composição.],
)

= Listas
- Use a faixa de opções para inserir tabelas, equações e referências.
- Pressione `Ctrl+S` para salvar e `Ctrl+E` para exportar o PDF.
"#;
