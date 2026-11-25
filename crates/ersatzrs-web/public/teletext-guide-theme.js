document.addEventListener("input", (event) => {
  const form = event.target.closest("[data-guide-theme]");
  if (!form) return;
  const banner = form.querySelector("[data-banner]");
  banner.textContent = form.elements.banner.value;
  banner.style.color = form.elements.accent.value;
  form.querySelector("[data-heading]").style.color =
    form.elements.heading.value;
});

// A library save in the page editor keeps the current channel draft and dialog.
document.addEventListener("submit", async (event) => {
  const form = event.target.closest("[data-theme-dialog] [data-guide-theme]");
  if (!form) return;
  event.preventDefault();
  const feedback = form.querySelector("[role=status]");
  const submit = form.querySelector("button");
  submit.disabled = true;
  try {
    const response = await fetch(form.action, {
      method: "POST",
      body: new URLSearchParams(new FormData(form)),
    });
    if (
      !response.ok ||
      new URL(response.url).searchParams.get("theme") !== "saved"
    ) throw Error();
    feedback.textContent = feedback.dataset.saved;
    form.dispatchEvent(new CustomEvent("guide-theme-saved", { bubbles: true }));
  } catch {
    feedback.textContent = feedback.dataset.error;
  } finally {
    submit.disabled = false;
  }
});
