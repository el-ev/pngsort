import wasmInit, { wasm_main } from "./pkg/pngsort.js";
import fs from "fs";

(async () => {
    const wasmBytes = await fs.promises.readFile("./pkg/pngsort_bg.wasm");
    const wasm = await wasmInit({ module_or_path: wasmBytes });
    const config = {
        descending: false,
        sort_range: "RowMajor",
        sort_mode: "Untied",
        sort_channel: ["R", "G", "B"],
    };
    const sourcePng = await fs.promises.readFile("f.png");
    const input = new Uint8Array(sourcePng);
    const configStr = JSON.stringify(config);

    const output = wasm_main(configStr, input);
    console.log(output);

    await fs.promises.writeFile("f2.png", output);

})()