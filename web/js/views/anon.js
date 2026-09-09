/**
 * Вьюха «Анонимные сообщения»: настройка команды /anon отдельно для
 * каждого Discord-сервера, на котором установлен бот (канал ID физически
 * привязан к конкретному серверу, общей на весь бот настройки быть не может).
 */

Views.anon = {
  title: "Анонимные сообщения",
  subtitle: "Настройка команды /anon для каждого сервера",

  async mount(root) {
    await withLoading(root, 3, async () => {
      let guilds;
      try {
        guilds = await Api.getGuilds();
      } catch (err) {
        renderErrorState(root, err.message, () => Views.anon.mount(root));
        return;
      }
      this.renderShell(root, guilds);
    });
  },

  renderShell(root, guilds) {
    if (guilds.length === 0) {
      renderEmptyState(
        root,
        "Бот пока ни на одном сервере",
        "Пригласи бота на Discord-сервер, чтобы настроить анонимные сообщения."
      );
      return;
    }

    root.innerHTML = `
      <div class="panel">
        <div class="panel-header">
          <div>
            <h2>Сервер</h2>
            <div class="desc">Настройки индивидуальны для каждого Discord-сервера</div>
          </div>
        </div>
        <div class="form-row" style="border-bottom:none; padding-bottom:0;">
          <div class="form-row-label">
            <div class="name">Выбери сервер</div>
          </div>
          <div class="form-row-control">
            <select id="guild-select" class="anon-select">
              ${guilds.map((g) => `<option value="${escapeHtml(g.id)}">${escapeHtml(g.name)}</option>`).join("")}
            </select>
          </div>
        </div>
      </div>
      <div id="anon-settings-container"></div>
    `;

    const select = root.querySelector("#guild-select");
    const container = root.querySelector("#anon-settings-container");
    const loadForGuild = () => this.loadSettings(container, select.value);
    select.addEventListener("change", loadForGuild);
    loadForGuild();
  },

  async loadSettings(container, guildId) {
    await withLoading(container, 4, async () => {
      let settings;
      let channels;
      let roles;
      try {
        [settings, channels, roles] = await Promise.all([
          Api.getAnonSettings(guildId),
          Api.getGuildChannels(guildId),
          Api.getGuildRoles(guildId),
        ]);
      } catch (err) {
        renderErrorState(container, err.message, () => this.loadSettings(container, guildId));
        return;
      }
      this.renderSettings(container, guildId, settings, channels, roles);
    });
  },

  renderSettings(container, guildId, settings, channels, roles) {
    const channelOptions = channels
      .map(
        (c) =>
          `<option value="${escapeHtml(c.id)}" ${c.id === settings.channel_id ? "selected" : ""}>#${escapeHtml(c.name)}</option>`
      )
      .join("");

    const roleRows = roles
      .map(
        (r) => `
        <label class="anon-role-row">
          <input type="checkbox" value="${escapeHtml(r.id)}" ${settings.allowed_role_ids.includes(r.id) ? "checked" : ""} />
          <span>${escapeHtml(r.name)}</span>
        </label>
      `
      )
      .join("");

    container.innerHTML = `
      <div class="panel">
        <div class="panel-header">
          <div><h2>Настройки на этом сервере</h2></div>
        </div>

        <div class="form-row">
          <div class="form-row-label">
            <div class="name">Модуль включён</div>
            <div class="desc">Полностью выключает /anon на этом сервере</div>
          </div>
          <div class="form-row-control">
            <label class="toggle">
              <input type="checkbox" id="anon-enabled" ${settings.enabled ? "checked" : ""} />
              <span class="track"></span>
            </label>
          </div>
        </div>

        <div class="form-row">
          <div class="form-row-label">
            <div class="name">Канал для анонимных сообщений</div>
            <div class="desc">Куда бот публикует сообщения из /anon</div>
          </div>
          <div class="form-row-control">
            <select id="anon-channel" class="anon-select">
              <option value="">— не выбран —</option>
              ${channelOptions}
            </select>
          </div>
        </div>

        <div class="form-row">
          <div class="form-row-label">
            <div class="name">Кулдаун</div>
            <div class="desc">Минимум секунд между анонимными сообщениями одного пользователя</div>
          </div>
          <div class="form-row-control">
            <input type="number" id="anon-cooldown" min="0" max="86400" value="${settings.cooldown_seconds}" />
          </div>
        </div>

        <div class="form-row">
          <div class="form-row-label">
            <div class="name">Кому разрешено</div>
            <div class="desc">Только выбранные роли смогут использовать /anon на этом сервере</div>
          </div>
          <div class="form-row-control">
            ${
              roles.length === 0
                ? '<div class="chip-hint">На сервере нет доступных ролей</div>'
                : `<div id="anon-roles" class="anon-role-list">${roleRows}</div>`
            }
          </div>
        </div>
      </div>

      <div style="display:flex; gap:10px; align-items:center;">
        <button class="btn btn-primary" type="button" id="save-anon">Сохранить</button>
        <span id="anon-save-status" style="color: var(--text-dim); font-size: 12.5px;"></span>
      </div>
    `;

    container.querySelector("#save-anon").addEventListener("click", async (e) => {
      const statusEl = container.querySelector("#anon-save-status");
      const button = e.target;

      const selectedRoles = Array.from(
        container.querySelectorAll('#anon-roles input[type="checkbox"]:checked')
      ).map((el) => el.value);

      const payload = {
        enabled: container.querySelector("#anon-enabled").checked,
        channel_id: container.querySelector("#anon-channel").value || null,
        allowed_role_ids: selectedRoles,
        cooldown_seconds: Number(container.querySelector("#anon-cooldown").value) || 0,
      };

      button.disabled = true;
      statusEl.textContent = "Сохраняю…";
      try {
        await Api.saveAnonSettings(guildId, payload);
        statusEl.textContent = "Сохранено.";
        App.toast("Настройки анонимных сообщений сохранены", "success");
      } catch (err) {
        statusEl.textContent = "";
        App.toast(err.message, "error");
      } finally {
        button.disabled = false;
      }
    });
  },
};