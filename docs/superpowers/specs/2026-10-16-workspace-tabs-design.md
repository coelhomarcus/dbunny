# Workspace document tabs

Each database connection keeps its own tab in the title bar. Inside that connection, every screen you open is a document tab, so several tables, a structure view, and several queries can stay open together.

## Two levels

Connection tabs do not change. One connection is one database. Several connections stay open at the same time. Switching a connection tab hides that whole connection, documents included, and shows it again as it was. Hidden connection panes stay mounted.

Picking another database in the sidebar switcher still opens a new connection tab, with that database's own documents. It does not replace the documents of the connection you were in.

Document tabs live in a bar at the top of the connection's content area. That bar is visually lighter than the connection tabs. The sidebar is no longer a page menu.

## Documents

A document has an id, a kind, a title, and the identity used to find it again.

| Kind | How many | Identity |
| --- | --- | --- |
| Query | Many | Its own id. A saved query also carries its saved id. |
| Data | One per relation | `data` + schema + name. Tables, views, materialized views, and partitions. |
| Structure | One per relation | `structure` + schema + name. Separate from the data tab. |
| Activity | One | `activity` |
| Roles | One | `roles` |
| Function or procedure | One per signature | kind + schema + name + argument list |
| Sequence, type, index, trigger | One per object | kind + schema + name |

Opening an identity that already exists focuses that tab. It does not create a second one.

A new query always creates a tab. The first one in a connection is titled `Query 1` and starts with `SELECT 1;`, as the editor does today. Later ones are `Query 2`, `Query 3`, and so on. Numbers only increase while the connection is open. A saved query uses its saved name. Opening a saved query that is already open focuses that tab and leaves its text alone.

Activity and Roles exist only when the engine has those screens. The menu does not offer them otherwise.

Documents are not saved to disk. Disconnecting drops that connection's list. Connecting again starts from one query. Switching connection tabs does not drop the list, because the connection pane stays mounted.

## What stays mounted

Only the front document is visible. The others stay mounted and hidden. A hidden document is `hidden` and `inert`: it gets no keyboard and no clicks. Scroll position, filters, a half-edited cell, query text, and query results survive a switch.

A query that is still running when you leave its tab runs to the end. The result is there when you come back.

The route no longer chooses the screen inside a connection. The connect page and the connected workspace stay as the two routes. Inside the workspace, each screen receives its document (schema, name, and, for a function, the argument list) as props. An `openDocument` on the connection replaces navigations that today jump to another screen. State that belongs to one screen stays in that mounted screen.

The list of documents and which one is in front is plain data, held for that connection and updated only through the functions below. The bar and the tree call those functions. They do not invent a second set of rules.

## Tab bar

Each tab shows an icon, a name, and a close button. The close button is always there, including on the last tab.

Icons match the explorer, so a tab and its row use the same picture:

- Table, view, materialized view, and partition use that row's icon. A partition uses the split icon.
- Structure uses the columns icon, so it is not mistaken for the data tab.
- Query uses the terminal icon. Activity, Roles, function, procedure, sequence, type, index, and trigger use the icons they already have.

The object's name comes first. The schema is shown smaller beside it, so `comments` in `public` and `comments` in `audit` can both be open. Queries have no schema. Activity and Roles are titled `Activity` and `Roles`.

A query with unsaved text keeps the dot it has today. Double-click renames a query tab and, when the query is saved, renames the saved query too. Other tabs do not rename.

`+` opens a new query. A menu beside it opens Activity and Roles when the engine has them. Choosing one that is already open focuses it.

When the tabs do not fit, the bar scrolls sideways and a menu lists the open tabs so one can be focused.

When nothing is open, the content area is empty except for a short line and a **New query** button. The bar still shows `+` and, when relevant, the Activity and Roles menu.

The **Data | Structure** control is removed from the table header and from the structure header.

## Sidebar

The sidebar keeps the database switcher and the schema tree. The Query, Activity, and Roles links at the top are removed.

Clicking a table, view, materialized view, or partition opens or focuses its data tab. Clicking a function, procedure, sequence, type, index, or trigger opens or focuses that object's tab.

The context menu keeps **Open data** and **Open structure** for tables, views, and materialized views, which already have those items. **Open structure** is the way to open a structure tab. Both items open or focus. They do not replace the other view.

The tree row lights when the front tab is that object. A structure tab lights the same row as the data tab. A query, Activity, or Roles tab lights no row.

Creating a table from the tree opens the new table's data tab, as it does today.

## Opening and updating

These actions go through the document list:

- **Tree click** and **Open data** focus an existing data tab without changing its filters, scroll, or edits.
- A **foreign-key link**, from the grid or from the structure screen, opens or focuses that table's data tab and applies the link's filters. Filters already on that tab are replaced, because the click means "show this row". If the tab has unsaved row edits, the existing discard question runs first. Cancelling still leaves the tab in front, with its old filters.
- **Open SQL**, from create table or from a structure change, opens a new query tab with that text and does not run it. It does not write into the query that was already open.
- The **history panel** stays on the query screen. Opening a saved query from it focuses the existing tab when that saved id is open, and otherwise opens a new tab with the saved name and text.
- **Rename table** renames the open data and structure tabs for that schema and name. They stay in front. Their identity becomes the new name.
- **Rename schema** rewrites the schema on every open document in that schema. The tabs stay open, and a document that was in front stays in front.
- **Drop table** closes the data and structure tabs for that relation. Index, trigger, and other object tabs close only when that object is dropped, or when their schema is dropped.
- **Drop schema** closes every document in that schema.
- After a drop, if the front tab closed, the next tab to the right takes focus. If the closed tab was the rightmost, the one on its left takes focus. If none remain, the area is empty.
- **Truncate** does not close the data tab. If that tab is open, it reloads, using the same discard question as refresh.
- **Duplicate** does not open a tab.

Rename and drop change the document list only after the engine accepts the change. A failure leaves the tabs as they were.

## Closing and failures

Closing a document tab does not ask. Unsaved query text and unsaved row edits are discarded, the same as leaving the page does today. The dot on a query is the warning.

The shortcut and the menu that close a connection tab still close the connection tab. They do not close the document in front.

If a table or an object fails to load, its tab stays open and shows the error that screen already shows. The tab does not close itself. A query error stays in that query's results.

## Module and tests

`web/src/lib/documents.ts` owns the list. It is pure: no React and no router. Tests live in `web/test/lib/documents.test.ts`.

The state is `{ items, activeId, nextQuery }`. `activeId` is `null` when the list is empty. `nextQuery` starts at 1 and is the number used for the next `Query N` title.

Each item is `{ id, kind, title, schema?, name?, detail?, savedId?, icon?, filterRequest? }`. `icon` is only for data, one of `table`, `view`, `matview`, `partition`. `detail` is only for a function or procedure, and it is the argument list. `filterRequest` is `{ id, filters }` where `filters` is the table's existing `TableFilter[]`. For a relation or an object, `title` is the object's name. For Activity and Roles, `title` is that fixed word. For a query, `title` is the label.

The functions are:

- `openDocument(state, target) -> state`. Targets are a new query (`sql?`, `savedId?`, `title?`), data (`schema`, `name`, `icon`, `filters?`), structure (`schema`, `name`), activity, roles, or an object (`kind`, `schema`, `name`, `detail?`). A new data tab opened with filters starts with filter request id 1. Focusing an existing data tab with filters replaces that request with the next id. Focusing it without filters leaves the request as it was.
- `closeDocument(state, id) -> state`. Focus moves as in **Opening and updating**. Closing an unknown id leaves the state unchanged.
- `renameRelation(state, schema, name, newName) -> state`. Sets `name` and `title` to `newName` on the data and structure documents for that relation.
- `renameSchema(state, schema, newName) -> state`. Sets `schema` to `newName` on every document whose schema was the old one. Queries, Activity, and Roles have no schema, so they stay.
- `dropRelation(state, schema, name) -> state`. Removes data and structure documents for that relation.
- `dropSchema(state, schema) -> state`. Removes every document whose schema is that schema.
- `seedQuery(state) -> state`. If `items` is empty, opens `Query 1` with `SELECT 1;`. Otherwise returns the same state.

Tests cover:

- Opening the same table again focuses the data tab. Opening its structure adds a second tab.
- A new query always adds a tab. A saved query that is already open is focused and keeps its text.
- Activity and Roles are opened at most once.
- Closing the front tab focuses the neighbor described above. Closing the last tab leaves `items` empty and `activeId` null.
- Renaming a table renames its data and structure tabs. Dropping it removes both. Dropping a schema removes every document in it.
- A foreign-key open focuses the data tab and records the link's filters on a new filter request.

The type check is part of the same work. There is no Rust change. A pass through the desktop app comes after the implementation, not as a unit test.

## Out of scope

- Remembering open documents after a restart or a disconnect.
- A confirm dialog when closing a document that has unsaved edits.
- Binding the connection-tab close shortcut to the document tab.
