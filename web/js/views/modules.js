/**
 * Вьюха «Модули»: глобальные тумблеры функционала бота. Хранятся внутри
 * того же config.json (поле modules), поэтому переиспользуют существующий
 * GET/PUT /api/config — отдельного API не потребовалось.
 */

Views.modules = {
  title: "Модули",
  subtitle: "Включение и отключение функционала бота целиком",

  async mount(root) {
    await withLoading(root, 3, async () => {
      let config;
      try {
        config = await Api.getConfig();
      } catch (err) {
        renderErrorState(root, err.message, () => Views.modules.mount(root));
        return;
      }
      this.render(root, config);
    });
  },

  moduleList: [
    {
      key: "text_reactions",
      name: "Реакции на упоминание",
      desc: "Бот отвечает заданными фразами, когда его тегнули в сообщении",
    },
    {
      key: "voice_commands",
      name: "Голосовые команды",
      desc: "Бот заходит в голосовой канал и включает звук по фразе",
    },
    {
      key: "anonymous_messages",
      name: "Анонимные сообщения",
      desc: "Команда /anon — отправка сообщений в канал без раскрытия автора",
    },
  ],

  render(root, config) {
    const modules = config.modules || {};

    const rows = this.moduleList
      .map(
        (m) => `
        <div class="form-row">
          <div class="form-row-label">
            <div class="name">${escapeHtml(m.name)}</div>
            <div class="desc">${escapeHtml(m.desc)}</div>
          </div>
          <div class="form-row-control">
            <label class="toggle">
              <input type="checkbox" data-module="${m.key}" ${modules[m.key] ? "checked" : ""} />
              <span class="track"></span>
            </label>
          </div>
        </div>
      `
      )
      .join("");

    root.innerHTML = `
      <div class="panel">
        <div class="panel-header">
          <div>
            <h2>Функционал бота</h2>
            <div class="desc">Выключенный модуль перестаёт реагировать сразу, без перезапуска</div>
          </div>
        </div>
        ${rows}
      </div>
      <div style="display:flex; gap:10px; align-items:center;">
        <button class="btn btn-primary" type="button" id="save-modules">Сохранить изменения</button>
        <span id="save-status" style="color: var(--text-dim); font-size: 12.5px;"></span>
      </div>
    `;

    root.querySelectorAll('input[type="checkbox"][data-module]').forEach((input) => {
      input.addEventListener("change", () => {
        // Мгновенно сохраняем — так тумблер ведёт себя предсказуемо, как
        // физический выключатель, а не как часть формы, которую можно забыть сохранить
      });
    });

    root.querySelector("#save-modules").addEventListener("click", async (e) => {
      const saveStatus = root.querySelector("#save-status");
      const button = e.target;
      const updatedModules = { ...modules };
      root.querySelectorAll('input[type="checkbox"][data-module]').forEach((input) => {
        updatedModules[input.dataset.module] = input.checked;
      });

      button.disabled = true;
      saveStatus.textContent = "Сохраняю…";
      try {
        await Api.saveConfig({ ...config, modules: updatedModules });
        saveStatus.textContent = "Сохранено.";
        App.toast("Настройки модулей сохранены", "success");
      } catch (err) {
        saveStatus.textContent = "";
        App.toast(err.message, "error");
      } finally {
        button.disabled = false;
      }
    });
  },
};