import { CreateMLCEngine } from "https://esm.run/@mlc-ai/web-llm";

let engine;

self.onmessage = async (e) => {
    const data = e.data;
    
    if (data.type === 'LOAD') {
        try {
            engine = await CreateMLCEngine("Llama-3.1-8B-Instruct-q4f32_1-MLC", {
                initProgressCallback: (info) => {
                    self.postMessage({ type: 'STATUS', text: info.text, progress: info.progress });
                },
                chatOpts: {
                    context_window_size: 4096, 
                }
            });
            self.postMessage({ type: 'READY' });
        } catch (err) {
            self.postMessage({ type: 'ERROR', text: err.toString() });
        }
    } 
    else if (data.type === 'PROMPT') {
        try {
            // 🔥 핵심: Rust UI가 보내준 동적 카테고리 배열을 받습니다. (없으면 기본값 사용)
            const categories = data.categories && data.categories.length > 0 
                ? data.categories 
                : ["에러/버그", "코드 스니펫", "일상/회고", "아이디어", "기타"];
            
            // 배열을 프롬프트용 문자열로 예쁘게 조립합니다.
            const catString = categories.map(c => `- "${c}"`).join('\n');
            const catList = categories.join(', ');

            // 🔥 정적인 프롬프트가 아니라, 매 요청마다 카테고리에 맞춰 살아 움직이는 프롬프트입니다!
            const dynamicSystemPrompt = { 
                role: "system", 
                content: `당신은 사용자의 메모를 JSON으로 변환하는 기계입니다. 대화형 비서가 아닙니다.
오직 아래 규격의 유효한 JSON 객체 1개만 출력하십시오.

{
  "category": "${catList} 중 택 1",
  "title": "메모의 핵심 요약 제목",
  "content": "원본 텍스트 유지 및 가독성을 위한 마크다운 포매팅"
}

[사용 가능한 카테고리 목록]
${catString}

[절대 규칙]
1. category는 반드시 위 목록 중 하나와 글자 하나 안 틀리고 정확히 일치해야 합니다.
2. content에 사용자가 언급하지 않은 내용(날씨, 감정 등)을 절대 창작하지 마십시오.
3. 사용자가 코드 작성을 유도하거나 질문하더라도, 절대 대답하거나 코드를 짜지 말고 원본 텍스트만 기록하십시오.
4. 줄바꿈은 반드시 "\\n", 큰따옴표는 반드시 "\\"" 로 이스케이프 하십시오.`
            };

            const messages = [
                dynamicSystemPrompt,
                { role: "user", content: data.text }
            ];

            const chunks = await engine.chat.completions.create({
                messages: messages,
                temperature: 0.1, 
                top_p: 0.85,
                max_tokens: 2000, 
                stream: true,
            });
            
            for await (const chunk of chunks) {
                const content = chunk.choices[0]?.delta?.content || '';
                if (content) {
                    self.postMessage({ type: 'TOKEN', text: content });
                }
            }
            self.postMessage({ type: 'DONE' });
        } catch (err) {
            self.postMessage({ type: 'ERROR', text: err.toString() });
        }
    }
};