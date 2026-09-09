/**
 * Вьюха «Обзор»: статус бота, пинг, число серверов, аптайм.
 */


Views.dashboard = {
  title: "Обзор",
  subtitle: "Текущее состояние бота",

  async mount(root) {
    await withLoading(root, 3, async () => {
      let status;
      try {
        status = await Api.getStatus();
      } catch (err) {
        renderErrorState(root, err.message, () => Views.dashboard.mount(root));
        return;
      }
      this.render(root, status);
    });
  },

  render(root, status) {
    const online = !!status.online;

    root.innerHTML = `
      <div class="panel">
        <div class="panel-header">
          <div>
            <h2>Состояние подключения</h2>
            <div class="desc">Данные обновляются при открытии вкладки</div>
          </div>
          <button class="btn btn-sm" type="button" id="refresh-status">Обновить</button>
        </div>
        <div class="stat-grid">
          <div class="stat-card">
            <div class="label">Статус</div>
            <div class="value ${online ? "ok" : "danger"}">${online ? "онлайн" : "офлайн"}</div>
          </div>
          <div class="stat-card">
            <div class="label">Пинг</div>
            <div class="value">${status.ping_ms !== null && status.ping_ms !== undefined ? status.ping_ms + " мс" : "—"}</div>
          </div>
          <div class="stat-card">
            <div class="label">Серверов</div>
            <div class="value">${status.guild_count ?? "—"}</div>
          </div>
          <div class="stat-card">
            <div class="label">Аптайм</div>
            <div class="value">${formatDuration(status.uptime_seconds)}</div>
          </div>
        </div>
      </div>

      <div class="panel">
        <div class="panel-header">
          <div>
            <h2>Быстрые ссылки</h2>
            <div class="desc">Частые задачи в один клик</div>
          </div>
        </div>
        <div style="display:flex; gap:10px; flex-wrap:wrap;">
          <button class="btn btn-sm" type="button" data-goto="settings">Настройки бота</button>
          <button class="btn btn-sm" type="button" data-goto="phrases">Фразы и реакции</button>
          <button class="btn btn-sm" type="button" data-goto="voice-commands">Голосовые команды</button>
          <button class="btn btn-sm" type="button" data-goto="sounds">Загрузить звук</button>
        </div>
      </div>
    `;

    root.querySelector("#refresh-status").addEventListener("click", () => this.mount(root));
    root.querySelectorAll("[data-goto]").forEach((btn) => {
      btn.addEventListener("click", () => App.navigate(btn.dataset.goto));
    });
  },
};