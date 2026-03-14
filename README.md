<p align="center">
  <img src="web/public/icon.svg" width="120" alt="DBunny logo" />
</p>

<h1 align="center">DBunny</h1>

<p align="center">Um cliente desktop leve e rapido para bancos de dados PostgreSQL.</p>

<p align="center">
  <a href="https://github.com/coelhomarcus/dbunny/releases/latest"><img src="https://img.shields.io/github/v/release/coelhomarcus/dbunny?sort=semver&label=&logo=github&labelColor=gray&color=gray" alt="Latest Release" /></a>
  <a href="https://github.com/coelhomarcus/dbunny/stargazers"><img src="https://img.shields.io/github/stars/coelhomarcus/dbunny?label=&logo=github&labelColor=gray&color=gray" alt="Stars" /></a>
</p>

<p align="center">
  <a href="README_EN.md">English</a>
</p>

<p align="center">
  <img src="https://marcuscoelho.com/assets/projects/dbunny.webp" width="720" alt="DBunny screenshot" />
</p>

## Sobre

DBunny e um cliente PostgreSQL desktop construido com Tauri e React. Oferece uma interface limpa com tema escuro para gerenciar conexoes, navegar schemas, editar dados e executar queries, tudo a partir de uma aplicacao nativa.

Disponivel para **Windows**, **macOS** (Intel e Apple Silicon) e **Linux**.

## Funcionalidades

### Gerenciamento de Conexoes
- Salve, edite e organize multiplas conexoes com nomes e cores personalizadas
- Importe conexoes a partir de uma URL PostgreSQL (`postgresql://user:pass@host:5432/db`)
- Suporte a SSL/TLS
- Armazenamento de credenciais criptografado (AES-GCM com derivacao de chave PBKDF2)
- Detalhes da conexao ocultados por padrao para privacidade

### Editor de Queries
- Editor SQL multi-abas com Monaco Editor e syntax highlighting
- Execute queries com `Ctrl+Enter` (texto selecionado ou conteudo completo do editor)
- Autocomplete SQL com mais de 40 palavras-chave
- Safe Mode ativado por padrao, detecta e bloqueia queries destrutivas antes da execucao:
  - `DROP DATABASE`, `DROP SCHEMA`, `DROP TABLE`
  - `TRUNCATE`, `DELETE` sem `WHERE`, `UPDATE` sem `WHERE`
  - `ALTER TABLE DROP COLUMN`
- Importacao e exportacao de arquivos `.sql`
- Abas de query persistem durante a navegacao

### Visualizacao e Edicao de Dados
- Navegue pelos dados com paginacao e ordenacao server-side
- Edicao inline de celulas com rastreamento de alteracoes pendentes
- Exclusao em lote de linhas com selecao multipla
- Destaque visual para celulas modificadas
- Colunas redimensionaveis
- Atalhos de teclado: `Ctrl+S` para salvar, `Escape` para cancelar

### Explorador de Schemas
- Arvore hierarquica de schemas, tabelas, views e funcoes
- Carregamento lazy para navegacao rapida
- Visualizacao da estrutura de tabelas (colunas, tipos, nulabilidade, valores padrao, chaves primarias)
- Inspecao de funcoes PostgreSQL (codigo fonte, linguagem, volatilidade, argumentos)

## Tecnologias

| Camada | Tecnologias |
|--------|-------------|
| Frontend | React 19, TypeScript, Tailwind CSS 4, Monaco Editor, TanStack Table |
| Backend | Tauri 2 (Rust), tokio-postgres, deadpool-postgres |
| Seguranca | AES-GCM, PBKDF2, SHA-256 |
| Build | Vite, GitHub Actions (CI/CD multiplataforma) |

## Como Rodar

**Requisitos:** Node.js, npm e [Rust](https://www.rust-lang.org/tools/install)

```bash
# instalar dependencias
npm install

# rodar em modo dev
npm run dev
```

Para build de producao:

```bash
npm run build
```

Ou baixe um binario pronto na [pagina de releases](https://github.com/coelhomarcus/dbunny/releases/latest).

## Creditos

- Logo: [SVG Repo — Bunny](https://www.svgrepo.com/svg/452876/bunny)
