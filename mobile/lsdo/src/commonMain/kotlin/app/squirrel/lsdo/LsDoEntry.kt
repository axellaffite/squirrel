package app.squirrel.lsdo

/**
 * One kind of screen in the app's LS-DO catalog: its [name] (the id it registers under), what it
 * is and how to reach it ([description]), and its [spec]. The catalog is built from these alone,
 * so it lists every screen, shown or not, and cannot drift from the code.
 */
class LsDoEntry(val name: String, val description: String, val spec: LsDoSpec<*, *>)
