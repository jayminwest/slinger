(() => {
  const fail = (code, message) => Object.assign(new Error(message), { code });
  const post = (m) => window.webkit.messageHandlers.slinger.postMessage(JSON.stringify(m)).catch((e) => {
    const text = String((e && e.message) || e);
    const at = text.indexOf(": ");
    throw at < 0 ? fail("unavailable", text) : fail(text.slice(0, at), text.slice(at + 2));
  });
  let context = null;
  const slinger = Object.freeze({
    v: 1,
    context: () => (context = context || post({ v: 1, call: "context" }).then((t) => Object.freeze(JSON.parse(t)))),
    reply: (value) => {
      let text;
      try { text = JSON.stringify(value); } catch (e) { return Promise.reject(fail("invalid", String(e.message))); }
      if (text === undefined) return Promise.reject(fail("invalid", "the value is not JSON"));
      return post({ v: 1, call: "reply", value: text }).then(() => undefined);
    },
  });
  Object.defineProperty(window, "slinger", { value: slinger, enumerable: true });
})();
