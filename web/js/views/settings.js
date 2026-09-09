/**
 * Вьюха «Настройки»: редактирование config.json бота.
 */


Views.settings = {
  title: "Настройки",
  subtitle: "Параметры поведения бота (config.json)",

  async mount(root) {
    await withLoading(root, 4, async () => {
      let config;
      try {
        config = await Api.getConfig();
      } catch (err) {
        renderErrorState(root, err.message, () => Views.settings.mount(root));
        return;
      }
      this.render(root, config);
    });
  },

  render(root, config) {
    root.innerHTML = `
      <div class="panel">
        <div class="panel-header">
          <div>
            <h2>Общие параметры</h2>
            <div class="desc">Изменения применяются после сохранения</div>
          </div>
        </div>

        <div class="form-row">
          <div class="form-row-label">
            <div class="name">Префикс команд</div>
            <div class="desc">Используется для !pause, !resume и т.д.</div>
          </div>
          <div class="form-row-control">
            <input type="text" id="f-prefix" maxlength="4" value="${escapeHtml(config.command_prefix)}" />
          </div>
        </div>

        <div class="form-row">
          <div class="form-row-label">
            <div class="name">Порог похожести фраз</div>
            <div class="desc">Ниже — бот узнаёт больше формулировок, но чаще ошибается</div>
          </div>
          <div class="form-row-control">
            <div class="slider-row">
              <input type="range" id="f-threshold" min="0.4" max="1" step="0.01" value="${config.similarity_threshold}" />
              <span class="slider-value" id="f-threshold-value">${config.similarity_threshold.toFixed(2)}</span>
            </div>
          </div>
        </div>

        <div class="form-row">
          <div class="form-row-label">
            <div class="name">Требовать упоминание</div>
            <div class="desc">Реакции и голосовые команды срабатывают только по тегу бота</div>
          </div>
          <div class="form-row-control">
            <label class="toggle">
              <input type="checkbox" id="f-mention" ${config.require_mention_for_reactions ? "checked" : ""} />
              <span class="track"></span>
            </label>
          </div>
        </div>

        <div class="form-row">
          <div class="form-row-label">
            <div class="name">Папка со звуками</div>
            <div class="desc">Относительно исполняемого файла бота</div>
          </div>
          <div class="form-row-control">
            <input type="text" id="f-sounds-dir" value="${escapeHtml(config.sounds_dir)}" />
          </div>
        </div>
      </div>

      <div style="display:flex; gap:10px; align-items:center;">
        <button class="btn btn-primary" type="button" id="save-config">Сохранить изменения</button>
        <span id="save-status" style="color: var(--text-dim); font-size: 12.5px;"></span>
      </div>
    `;

    const thresholdInput = root.querySelector("#f-threshold");
    const thresholdValue = root.querySelector("#f-threshold-value");
    thresholdInput.addEventListener("input", () => {
      thresholdValue.textContent = Number(thresholdInput.value).toFixed(2);
    });

    root.querySelector("#save-config").addEventListener("click", async () => {
      const saveStatus = root.querySelector("#save-status");
      const button = root.querySelector("#save-config");
      const payload = {
        command_prefix: root.querySelector("#f-prefix").value.trim() || "!",
        similarity_threshold: Number(thresholdInput.value),
        require_mention_for_reactions: root.querySelector("#f-mention").checked,
        sounds_dir: root.querySelector("#f-sounds-dir").value.trim() || "sounds",
      };

      button.disabled = true;
      saveStatus.textContent = "Сохраняю…";
      try {
        await Api.saveConfig(payload);
        saveStatus.textContent = "Сохранено.";
        App.toast("Настройки сохранены", "success");
      } catch (err) {
        saveStatus.textContent = "";
        App.toast(err.message, "error");
      } finally {
        button.disabled = false;
      }
    });
  },
};