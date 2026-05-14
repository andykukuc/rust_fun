import init, { start } from './pkg/wasm_site_1.js';

async function main() {
    await init();
    start("/sysinfo");
}

main();
