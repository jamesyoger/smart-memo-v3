
export function yield_event_loop() {
    return new Promise(resolve => setTimeout(resolve, 0));
}

export function spawn_worker() { 
    console.log('🟢 [메인] 1. 워커 생성 함수가 호출되었습니다.');
    
    let js_url = '';
    const links = document.querySelectorAll('link');
    for (let i = 0; i < links.length; i++) {
        if (links[i].href.includes('llama3_worker') && links[i].href.endsWith('.js')) {
            js_url = links[i].href;
            break;
        }
    }
    
    const wasm_url = js_url.replace('.js', '_bg.wasm');
    
    const workerCode = `
        console.log('🟡 [워커] 3. 워커 내부 공간이 열렸습니다. WASM 시동을 시작합니다.');
        import init from '${js_url}';
        
        init('${wasm_url}').then(() => {
            console.log('🟡 [워커] 4. WASM 엔진 시동 완벽 성공! 메인 스레드의 명령을 기다립니다.');
        }).catch(e => {
            console.error('🔴 [워커] WASM 시동 치명적 실패:', e);
        });
    `;
    const blob = new Blob([workerCode], { type: 'application/javascript' });
    const worker = new Worker(URL.createObjectURL(blob), { type: 'module' });
    return worker;
}
