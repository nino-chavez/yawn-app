/* Synthetic review bridge only. Production title filtering remains in Rust.
 * Match library_snapshot's filter.title, total, shown and filterActive contract. */
(() => {
  const invoke = window.__TAURI__.core.invoke;
  window.__TAURI__.core.invoke = async (command, args) => {
    const result = await invoke(command, args);
    if (command !== 'library_snapshot' || !Array.isArray(result?.rows)) return result;
    const title = String(args?.filter?.title || '').trim().toLocaleLowerCase();
    const rows = title ? result.rows.filter(row => String(row.label || '').toLocaleLowerCase().includes(title)) : result.rows;
    return { ...result, rows, total: result.rows.length, shown: rows.length, filterActive: Boolean(title) };
  };
})();
