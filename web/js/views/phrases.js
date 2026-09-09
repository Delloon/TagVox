/**
 * Вьюха «Фразы и реакции»: text_reactions (триггеры -> ответы) и
 * unknown_response (фраза-заглушка, когда бот не понял сообщение).
 */


Views.phrases = {
  title: "Фразы и реакции",
  subtitle: "Реакции срабатывают, когда бота упомянули в сообщении",

  async mount(root) {
    await withLoading(root, 5, async () => {
      let reactions;
      let unknown;
      try {
        [reactions, unknown] = await Promise.all([Api.getReactions(), Api.getUnknownResponses()]);
      } catch (err) {
        renderErrorState(root, err.message, () => Views.phrases.mount(root));
        return;
      }
      this.render(root, reactions, unknown);
    });
  },

  render(root, reactions, unknown) {
    root.innerHTML = `
      <div class="panel">
        <div class="panel-header">
          <div>
            <h2>Реакции</h2>
            <div class="desc">Количество не ограничено — бот выбирает наиболее похожий триггер</div>
          </div>
          <button class="btn btn-sm btn-primary" type="button" id="add-reaction">+ Добавить реакцию</button>
        </div>
        <div class="entry-list" id="reaction-list"></div>
      </div>

      <div class="panel">
        <div class="panel-header">
          <div>
            <h2>Фраза-заглушка</h2>
            <div class="desc">Отправляется, когда ни один триггер не подошёл</div>
          </div>
        </div>
        <div class="chip-field">
          <label>Варианты ответа (бот выбирает случайный)</label>
          <div class="chip-input" id="unknown-chip-input"></div>
          <div class="chip-hint">Enter или запятая — добавить вариант</div>
        </div>
        <div style="margin-top:14px;">
          <button class="btn btn-primary btn-sm" type="button" id="save-unknown">Сохранить</button>
        </div>
      </div>
    `;

    const listEl = root.querySelector("#reaction-list");
    if (reactions.length === 0) {
      renderEmptyState(listEl, "Реакций пока нет", "Добавь первую кнопкой выше — например, ответ на «привет».");
    } else {
      reactions.forEach((reaction) => listEl.appendChild(this.renderReactionCard(reaction)));
    }

    root.querySelector("#add-reaction").addEventListener("click", () => {
      if (listEl.querySelector(".state-block")) listEl.innerHTML = "";
      const card = this.renderReactionCard({ id: "", triggers: [], responses: [] }, true);
      listEl.prepend(card);
      card.querySelector(".chip-input input")?.focus();
    });

    const unknownChipInput = createChipInput(
      root.querySelector("#unknown-chip-input"),
      unknown,
      "Добавить вариант фразы…"
    );
    root.querySelector("#save-unknown").addEventListener("click", async (e) => {
      const items = unknownChipInput.getItems();
      if (items.length === 0) {
        App.toast("Нужен хотя бы один вариант фразы-заглушки", "error");
        return;
      }
      e.target.disabled = true;
      try {
        await Api.saveUnknownResponses(items);
        App.toast("Фраза-заглушка сохранена", "success");
      } catch (err) {
        App.toast(err.message, "error");
      } finally {
        e.target.disabled = false;
      }
    });
  },

  renderReactionCard(reaction, isNew = false) {
    const card = elFromHtml(`
      <div class="entry-card">
        <div class="entry-card-header">
          <span class="entry-id">${isNew ? "новая запись" : escapeHtml(reaction.id)}</span>
          <div class="entry-actions">
            <button class="btn btn-sm" type="button" data-action="save">Сохранить</button>
            <button class="btn btn-sm btn-danger" type="button" data-action="delete">Удалить</button>
          </div>
        </div>
        <div class="entry-body">
          ${
            isNew
              ? `<div class="chip-field"><label>Идентификатор (латиницей, без пробелов)</label>
                   <input type="text" class="entry-id-input" placeholder="greeting" style="width:100%; background:var(--bg); border:1px solid var(--border); border-radius:var(--radius); padding:8px 10px;" /></div>`
              : ""
          }
          <div class="chip-field">
            <label>Триггеры (похожие фразы)</label>
            <div class="chip-input" data-role="triggers"></div>
            <div class="chip-hint">Enter или запятая — добавить</div>
          </div>
          <div class="chip-field">
            <label>Варианты ответа</label>
            <div class="chip-input" data-role="responses"></div>
            <div class="chip-hint">Enter или запятая — добавить</div>
          </div>
        </div>
      </div>
    `);

    const triggersInput = createChipInput(card.querySelector('[data-role="triggers"]'), reaction.triggers, "Триггер…");
    const responsesInput = createChipInput(card.querySelector('[data-role="responses"]'), reaction.responses, "Ответ…");

    card.querySelector('[data-action="delete"]').addEventListener("click", async () => {
      if (isNew) {
        card.remove();
        return;
      }
      if (!confirm(`Удалить реакцию «${reaction.id}»?`)) return;
      try {
        await Api.deleteReaction(reaction.id);
        card.remove();
        App.toast("Реакция удалена", "success");
      } catch (err) {
        App.toast(err.message, "error");
      }
    });

    card.querySelector('[data-action="save"]').addEventListener("click", async (e) => {
      const triggers = triggersInput.getItems();
      const responses = responsesInput.getItems();

      if (triggers.length === 0 || responses.length === 0) {
        App.toast("Нужен хотя бы один триггер и один вариант ответа", "error");
        return;
      }

      e.target.disabled = true;
      try {
        if (isNew) {
          const idInput = card.querySelector(".entry-id-input");
          const id = (idInput.value.trim() || slugify(triggers[0])).toLowerCase();
          const created = await Api.createReaction({ id, triggers, responses });
          App.toast("Реакция создана", "success");
          const freshCard = this.renderReactionCard(created);
          card.replaceWith(freshCard);
        } else {
          await Api.updateReaction(reaction.id, { triggers, responses });
          App.toast("Реакция сохранена", "success");
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