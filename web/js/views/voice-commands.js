/**
 * Вьюха «Голосовые команды»: фразы, при упоминании которых бот заходит в
 * голосовой канал и включает привязанный локальный файл.
 */


Views.voiceCommands = {
  title: "Голосовые команды",
  subtitle: "Фраза при упоминании бота запускает привязанный аудиофайл",

  async mount(root) {
    await withLoading(root, 5, async () => {
      let commands;
      let sounds;
      try {
        [commands, sounds] = await Promise.all([Api.getVoiceCommands(), Api.getSounds()]);
      } catch (err) {
        renderErrorState(root, err.message, () => Views.voiceCommands.mount(root));
        return;
      }
      this.render(root, commands, sounds);
    });
  },

  render(root, commands, sounds) {
    root.innerHTML = `
      <div class="panel">
        <div class="panel-header">
          <div>
            <h2>Команды</h2>
            <div class="desc">Количество не ограничено</div>
          </div>
          <button class="btn btn-sm btn-primary" type="button" id="add-command">+ Добавить команду</button>
        </div>
        <div class="entry-list" id="command-list"></div>
      </div>
    `;

    const listEl = root.querySelector("#command-list");
    if (commands.length === 0) {
      renderEmptyState(
        listEl,
        "Голосовых команд пока нет",
        sounds.length === 0
          ? "Сначала загрузи хотя бы один звук на вкладке «Звуки», затем привяжи к нему фразу."
          : "Добавь первую кнопкой выше."
      );
    } else {
      commands.forEach((cmd) => listEl.appendChild(this.renderCommandCard(cmd, sounds)));
    }

    root.querySelector("#add-command").addEventListener("click", () => {
      if (sounds.length === 0) {
        App.toast("Сначала загрузи звук на вкладке «Звуки»", "error");
        return;
      }
      if (listEl.querySelector(".state-block")) listEl.innerHTML = "";
      const card = this.renderCommandCard({ id: "", triggers: [], file: sounds[0].name }, sounds, true);
      listEl.prepend(card);
      card.querySelector(".chip-input input")?.focus();
    });
  },

  renderCommandCard(cmd, sounds, isNew = false) {
    const soundOptions = sounds
      .map((s) => `<option value="${escapeHtml(s.name)}" ${s.name === cmd.file ? "selected" : ""}>${escapeHtml(s.name)}</option>`)
      .join("");
    const fileIsKnown = sounds.some((s) => s.name === cmd.file);

    const card = elFromHtml(`
      <div class="entry-card">
        <div class="entry-card-header">
          <span class="entry-id">${isNew ? "новая запись" : escapeHtml(cmd.id)}</span>
          <div class="entry-actions">
            <button class="btn btn-sm" type="button" data-action="save">Сохранить</button>
            <button class="btn btn-sm btn-danger" type="button" data-action="delete">Удалить</button>
          </div>
        </div>
        <div class="entry-body">
          ${
            isNew
              ? `<div class="chip-field"><label>Идентификатор (латиницей, без пробелов)</label>
                   <input type="text" class="entry-id-input" placeholder="sad_song" style="width:100%; background:var(--bg); border:1px solid var(--border); border-radius:var(--radius); padding:8px 10px;" /></div>`
              : ""
          }
          <div class="chip-field">
            <label>Триггеры (похожие фразы)</label>
            <div class="chip-input" data-role="triggers"></div>
            <div class="chip-hint">Enter или запятая — добавить</div>
          </div>
          <div class="chip-field">
            <label>Звуковой файл</label>
            <div class="entry-voice-file">
              <select data-role="file-select">${soundOptions}</select>
            </div>
            ${
              !fileIsKnown && cmd.file
                ? `<div class="chip-hint" style="color:var(--danger);">Файл «${escapeHtml(cmd.file)}» не найден среди загруженных звуков</div>`
                : ""
            }
          </div>
        </div>
      </div>
    `);

    const triggersInput = createChipInput(card.querySelector('[data-role="triggers"]'), cmd.triggers, "Триггер…");

    card.querySelector('[data-action="delete"]').addEventListener("click", async () => {
      if (isNew) {
        card.remove();
        return;
      }
      if (!confirm(`Удалить команду «${cmd.id}»?`)) return;
      try {
        await Api.deleteVoiceCommand(cmd.id);
        card.remove();
        App.toast("Команда удалена", "success");
      } catch (err) {
        App.toast(err.message, "error");
      }
    });

    card.querySelector('[data-action="save"]').addEventListener("click", async (e) => {
      const triggers = triggersInput.getItems();
      const file = card.querySelector('[data-role="file-select"]').value;

      if (triggers.length === 0) {
        App.toast("Нужен хотя бы один триггер", "error");
        return;
      }

      e.target.disabled = true;
      try {
        if (isNew) {
          const idInput = card.querySelector(".entry-id-input");
          const id = (idInput.value.trim() || slugify(triggers[0])).toLowerCase();
          const created = await Api.createVoiceCommand({ id, triggers, file });
          App.toast("Команда создана", "success");
          const freshCard = this.renderCommandCard(created, sounds);
          card.replaceWith(freshCard);
        } else {
          await Api.updateVoiceCommand(cmd.id, { triggers, file });
          App.toast("Команда сохранена", "success");
        }
      } catch (err) {
        App.toast(err.message, "error");
      } finally {
        e.target.disabled = false;
      }
    });

    return card;
  },
};