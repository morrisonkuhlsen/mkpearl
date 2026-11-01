//! The documents File › New starts from. A test compiles them all.

pub struct Template {
    pub name: &'static str,
    pub source: &'static str,
}

pub const TEMPLATES: [Template; 4] = [
    Template {
        name: "Artigo",
        source: r#"#set document(title: "Título do artigo")
#set page(paper: "a4", margin: 2.5cm, numbering: "1")
#set text(lang: "pt", size: 11pt)
#set par(justify: true)
#set heading(numbering: "1.1")

#align(center)[
  #text(18pt, weight: "bold")[Título do artigo]

  Nome da autoria \
  Instituição
]

*Resumo.* Escreva aqui um resumo do trabalho, com objetivo, método e principais resultados.

= Introdução
Apresente o problema e o objetivo do trabalho.

= Método
Descreva como o trabalho foi feito.

= Resultados
Mostre o que foi encontrado.

$ E = m c^2 $

= Conclusão
Resuma o que os resultados significam.
"#,
    },
    Template {
        name: "Relatório (ABNT)",
        source: r#"#set document(title: "Título do relatório")
#set page(paper: "a4", margin: (top: 3cm, left: 3cm, bottom: 2cm, right: 2cm))
#set text(lang: "pt", size: 12pt)
#set par(justify: true, leading: 1.05em, first-line-indent: 1.25cm)
#set heading(numbering: "1.1")

#align(center)[
  #upper[Nome da instituição]

  #v(1fr)
  #text(16pt, weight: "bold")[#upper[Título do relatório]]
  #v(1fr)

  Cidade \
  #datetime.today().year()
]
#pagebreak()

#outline(title: [Sumário])
#pagebreak()

= Introdução
Apresente o tema e os objetivos do relatório.

= Desenvolvimento
Desenvolva o conteúdo aqui.

= Conclusão
Apresente as conclusões.
"#,
    },
    Template {
        name: "Carta",
        source: r#"#set page(paper: "a4", margin: 3cm)
#set text(lang: "pt", size: 12pt)
#set par(justify: true)

#align(right)[Cidade, #datetime.today().display("[day]/[month]/[year]")]

#v(1cm)
À pessoa destinatária,

Escreva aqui o texto da carta.

#v(1cm)
Atenciosamente,

#v(1.5cm)
Seu nome
"#,
    },
    Template {
        name: "Currículo",
        source: r#"#set page(paper: "a4", margin: 2cm)
#set text(lang: "pt", size: 10.5pt)
#show heading: it => [
  #text(12pt, weight: "bold")[#it.body]
  #v(-0.6em)
  #line(length: 100%, stroke: 0.5pt)
]

#align(center)[
  #text(20pt, weight: "bold")[Seu nome]

  cidade · e-mail · telefone
]

= Resumo
Duas ou três linhas sobre sua experiência.

= Experiência
*Cargo*, Empresa #h(1fr) 2024 – hoje
- Principal resultado alcançado.

= Formação
*Curso*, Instituição #h(1fr) 2023

= Habilidades
Liste aqui as principais.
"#,
    },
];
