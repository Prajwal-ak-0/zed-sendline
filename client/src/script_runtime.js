// JetBrains response-handler API. The host provides data, never OS or network APIs.
(function (inputJson, source) {
    "use strict";
    const input = JSON.parse(inputJson);
    const values = Object.assign(Object.create(null), input.globals);
    const changes = [];
    const tests = [];
    const results = [];
    const logs = [];
    const exit = Symbol("client.exit");
    const clone = value => JSON.parse(JSON.stringify(value));
    const message = error => String(error) + (error && error.stack ? "\n" + error.stack : "");
    const log = (...args) => logs.push(args.map(value => {
        if (typeof value === "string") return value;
        try { return JSON.stringify(value); } catch (_) { return String(value); }
    }).join(" "));
    const checkSync = value => {
        if (value && typeof value.then === "function") {
            throw new Error("Async response handlers are not supported");
        }
    };
    const global = Object.freeze({
        set(name, value) {
            name = String(name);
            if (value === undefined) return this.clear(name);
            const saved = clone(value);
            values[name] = saved;
            changes.push({op: "set", name, value: clone(saved)});
        },
        get(name) { return values[String(name)]; },
        isEmpty() { return Object.keys(values).length === 0; },
        clear(name) {
            name = String(name);
            delete values[name];
            changes.push({op: "clear", name});
        },
        clearAll() {
            for (const name of Object.keys(values)) delete values[name];
            changes.push({op: "clearAll"});
        }
    });
    const client = Object.freeze({
        global,
        test(name, callback) {
            if (typeof callback !== "function") throw new TypeError("client.test requires a function");
            tests.push({name: String(name), callback});
        },
        assert(condition, description = "Assertion failed") {
            if (!condition) throw new Error(String(description));
        },
        log,
        exit() { throw exit; }
    });
    const response = {
        status: input.status,
        body: input.body,
        contentType: input.contentType,
        headers: Object.freeze({
            valueOf(name) { return this.valuesOf(name)[0] ?? null; },
            valuesOf(name) {
                return input.headers.filter(([key]) => key.toLowerCase() === String(name).toLowerCase())
                    .map(([, value]) => value);
            },
            all() { return input.headers.map(([name, value]) => ({name, value})); }
        })
    };
    let error = null;
    try {
        const handler = new Function("client", "response", "console", '"use strict";\n' + source);
        checkSync(handler(client, response, Object.freeze({log, info: log, warn: log, error: log, debug: log})));
    } catch (caught) {
        if (caught !== exit) error = message(caught);
    }
    // JetBrains runs registered tests after the handler, in registration order.
    for (const test of tests) {
        try {
            checkSync(test.callback());
            results.push({name: test.name, passed: true, error: null});
        } catch (caught) {
            results.push({name: test.name, passed: false, error: message(caught)});
        }
    }
    return JSON.stringify({tests: results, logs, error, changes});
})
