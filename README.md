# Pearl

Um editor de documentos em Rust: código Typst à esquerda, páginas ao vivo à direita, e uma faixa de opções no estilo de um processador de texto.

```
cargo run --release                 # abre o documento de exemplo, depois do splash
cargo run --release -- texto.typ    # abre um arquivo
cargo run --release -- --no-splash  # pula o splash
cargo test
cargo run --example shot -- saida.png Início   # desenha a janela fora da tela
```

A interface segue a do MK Matrix e o splash, o layout do MK Perth; veja `NOTICE`.
