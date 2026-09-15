/*
Adapted from Leptos 0.8.20 islands bootstrap (leptos/src/hydration/island_script.js).
https://github.com/leptos-rs/leptos/tree/30fa4e039455bea69c9467e8ec45cad7fce0722f

MIT License

Copyright (c) 2022 Greg Johnston

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
*/

// Leptos 0.8 islands bootstrap contract: hydrate parents before children, and
// restore the parent's children callback while traversing leptos-children.
// Required controls must start even when requestIdleCallback is never granted.
const script = document.getElementById("ersatzrs-hydration");
const childrenCallbacks = [];

async function hydrateIsland(element, id, module) {
    const hydrate = module[id];
    if (hydrate) {
        const children = childrenCallbacks[childrenCallbacks.length - 1];
        if (children) children();
        const result = hydrate(element);
        if (result && result.then) await result;
    } else {
        console.warn(`Could not find WASM function for the island ${id}.`);
    }
}

async function hydrateTree(node, module) {
    if (node.nodeType !== Node.ELEMENT_NODE) return;
    const tag = node.tagName.toLowerCase();
    if (tag === "leptos-island") {
        await hydrateIsland(node, node.dataset.component || null, module);
    }
    const isChildren = tag === "leptos-children";
    if (isChildren) {
        childrenCallbacks.push(node.$$on_hydrate);
        for (const child of node.children) await hydrateTree(child, module);
        childrenCallbacks.pop();
    } else {
        for (const child of node.children) await hydrateTree(child, module);
    }
}

// A module script runs after HTML parsing. Import and WASM initialization
// remain asynchronous; there is no optional idle gate before either starts.
window.__ersatzrs_hydration_phase = "loading-module";
try {
    const module = await import(script.dataset.js);
    window.__hydrateIsland = (element, id) => hydrateIsland(element, id, module);
    window.__ersatzrs_hydration_phase = "initializing-wasm";
    await module.default({ module_or_path: script.dataset.wasm });
    module.hydrate();
    window.__ersatzrs_hydration_phase = "hydrating-islands";
    await hydrateTree(document.body, module);
    window.__ersatzrs_hydration_phase = "ready";
} catch (error) {
    window.__ersatzrs_hydration_phase = "failed";
    throw error;
}
