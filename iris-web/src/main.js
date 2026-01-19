import init, { compile_to_wasm } from '../pkg/iris_lang.js';

let wasmInitialized = false;

const output = document.getElementById('output');
const codeEditor = document.getElementById('code-editor');
const compileBtn = document.getElementById('compile-btn');
const runBtn = document.getElementById('run-btn');
const clearBtn = document.getElementById('clear-btn');

function log(message, type = 'info') {
    const timestamp = new Date().toLocaleTimeString();
    const className = type === 'error' ? 'error' : type === 'success' ? 'success' : '';
    output.innerHTML += `<div class="${className}">[${timestamp}] ${message}</div>`;
    output.scrollTop = output.scrollHeight;
}

function clearOutput() {
    output.innerHTML = '';
    log('Output cleared');
}

async function initializeWasm() {
    try {
        log('Initializing WASM module...');
        await init();
        wasmInitialized = true;
        log('WASM module initialized successfully', 'success');
        return true;
    } catch (error) {
        log(`Failed to initialize WASM: ${error.message}`, 'error');
        return false;
    }
}

async function compileCode() {
    const code = codeEditor.value.trim();
    if (!code) {
        log('No code to compile', 'error');
        return;
    }

    if (!wasmInitialized) {
        log('WASM module not initialized yet', 'error');
        return;
    }

    compileBtn.disabled = true;
    log('Compiling Iris code...');

    try {
        const wasmBytes = compile_to_wasm(code);
        
        if (wasmBytes.length > 0 && wasmBytes[0] === 0x00 && wasmBytes[1] === 0x61 && wasmBytes[2] === 0x73) {
            log(`Compilation successful! Generated ${wasmBytes.length} bytes of WASM`, 'success');
            window.latestWasmBytes = wasmBytes;
            runBtn.disabled = false;
            
            const hexPreview = Array.from(wasmBytes.slice(0, 100))
                .map(b => b.toString(16).padStart(2, '0'))
                .join(' ');
            log(`WASM preview (first 100 bytes):`);
            log(hexPreview, 'code');
        } else {
            const errorMsg = new TextDecoder().decode(wasmBytes);
            if (errorMsg.startsWith('ERROR:')) {
                log(`Compilation error: ${errorMsg}`, 'error');
                runBtn.disabled = true;
            } else {
                log(`Compilation successful! Generated ${wasmBytes.length} bytes`, 'success');
                window.latestWasmBytes = wasmBytes;
                runBtn.disabled = false;
            }
        }
        
    } catch (error) {
        log(`Compilation failed: ${error.message}`, 'error');
        runBtn.disabled = true;
    } finally {
        compileBtn.disabled = false;
    }
}

async function runWasm() {
    if (!window.latestWasmBytes) {
        log('No compiled WASM available. Please compile first.', 'error');
        return;
    }

    runBtn.disabled = true;
    log('Running WASM...');

    try {
        const wasmModule = await WebAssembly.compile(window.latestWasmBytes);
        const wasmInstance = await WebAssembly.instantiate(wasmModule);
        
        if (wasmInstance.exports.main) {
            const result = wasmInstance.exports.main();
            log(`WASM main() returned: ${result}`, 'success');
        } else if (wasmInstance.exports._start) {
            wasmInstance.exports._start();
            log('WASM _start() executed successfully', 'success');
        } else {
            log('WASM loaded but no entry point found (main or _start)', 'error');
        }
        
        const exports = Object.keys(wasmInstance.exports);
        log(`Available exports: ${exports.join(', ')}`);
        
    } catch (error) {
        log(`WASM execution failed: ${error.message}`, 'error');
    } finally {
        runBtn.disabled = false;
    }
}

compileBtn.addEventListener('click', compileCode);
runBtn.addEventListener('click', runWasm);
clearBtn.addEventListener('click', clearOutput);

codeEditor.addEventListener('keydown', (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
        e.preventDefault();
        compileCode();
    }
});

document.addEventListener('DOMContentLoaded', async () => {
    log('Iris Web IDE loaded');
    runBtn.disabled = true;
    
    const initialized = await initializeWasm();
    if (initialized) {
        log('Ready to compile Iris code!', 'success');
        
        codeEditor.value = `// Example: Simple addition function
fn add(a: Int, b: Int) -> Int =: a + b

// Example: Pipeline calculation
let result
=: 42
:: * 2
:: + 10
// result should be 94`;
    }
});