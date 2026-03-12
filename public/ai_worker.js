console.log("🏁 [Worker] ai_worker.js 파일 파싱 완료! 🚀");

import { CreateMLCEngine } from "https://esm.run/@mlc-ai/web-llm";
import { pipeline, env } from "https://cdn.jsdelivr.net/npm/@xenova/transformers@2.17.2/dist/transformers.js";

console.log("📦 [Worker] 외부 AI 모듈 Import 성공!");

env.allowLocalModels = false;
env.backends.onnx.wasm.wasmPaths = 'https://cdn.jsdelivr.net/npm/@xenova/transformers@2.17.2/dist/';

const LLM_MODEL_ID = "Llama-3.2-1B-Instruct-q4f16_1-MLC";
const EMBED_MODEL_ID = "Xenova/paraphrase-multilingual-MiniLM-L12-v2";

let llmEngine = null;
let embedder = null;

function cosineSimilarity(vecA, vecB) {
    let dotProduct = 0, normA = 0, normB = 0;
    for (let i = 0; i < vecA.length; i++) {
        dotProduct += vecA[i] * vecB[i];
        normA += vecA[i] * vecA[i];
        normB += vecB[i] * vecB[i];
    }
    if (normA === 0 || normB === 0) return 0;
    return dotProduct / (Math.sqrt(normA) * Math.sqrt(normB));
}

function calculateKoreanHybridBoost(query, memoText) {
    let boostScore = 0;
    const queryWords = query.split(/[\s,!?]+/).filter(w => w.length > 0);
    
    queryWords.forEach(word => {
        if (memoText.includes(word)) {
            boostScore += 0.15; 
        } 
        else {
            for (let char of word) {
                if (!"은는이가을를에의도만하".includes(char) && memoText.includes(char)) {
                    boostScore += 0.05;
                }
            }
        }
    });
    return boostScore;
}

self.onmessage = async (event) => {
    const data = event.data;
    const command = data.msg_type || data.type;

    try {
        if (command === "LOAD") {
            self.postMessage({ type: "STATUS", text: "AI 엔진 초기화 시작 (다국어 모델)..." });
            try {
                llmEngine = await CreateMLCEngine(LLM_MODEL_ID, { 
                    initProgressCallback: (p) => {
                        self.postMessage({ type: "STATUS", text: `Llama 1B: ${p.text}`, progress: p.progress * 0.5 });
                    }
                });
                embedder = await pipeline('feature-extraction', EMBED_MODEL_ID, {
                    progress_callback: (p) => {
                        if (p.status === 'progress') {
                            self.postMessage({ type: "STATUS", text: `Vector Engine: 적재 중...`, progress: 0.5 + ((p.progress || 0) / 100) * 0.5 });
                        }
                    }
                });
                self.postMessage({ type: "READY" });
            } catch (initError) {
                console.error("🚨 모델 초기화 에러:", initError);
                self.postMessage({ type: "ERROR", text: `엔진 초기화 실패: ${initError.message || initError}` });
            }
        } 
        else if (command === "PROMPT_CLASSIFY") {
            const categories = data.categories;
            const txt = data.text;
            const txtTrim = txt.trim();
            
            // 🚀 [1단계] 만능 하이브리드 라우터 (201호 버그 완벽 픽스!)
            let forcedCategory = null;
            
            // 💡 방어막: 숫자 뒤에 방 번호, 층수, 날짜 등이 붙어있으면 돈으로 취급하지 않습니다.
            const hasRoomOrDate = /\d+\s*(?:호|층|동|번지|년|월|일|번)/.test(txt);
            
            // 지출 키워드가 있거나, 띄어쓰기 후 숫자로 끝나는 전형적인 영수증 포맷인지 확인합니다.
            const hasMoneyKeyword = /원|지출|결제|샀|비용|요금|식비|페이|가격|금액/.test(txt);
            const isReceiptPattern = txtTrim.length <= 50 && /\s\d{2,}(?:,\d{3})*(?:\s*원)?$/.test(txtTrim);

            // 1. 지출 판단: 방/날짜 번호가 아닐 때만 영수증으로 인정!
            if (!hasRoomOrDate && (hasMoneyKeyword || isReceiptPattern)) {
                forcedCategory = "지출";
            }
            // 2. 회의록 공식 (우선순위 최고)
            else if ((txt.includes("회의") || txt.includes("미팅") || txt.includes("프로젝트")) && (txt.includes("결과") || txt.includes("결정") || txt.includes("주제") || txt.includes("록"))) {
                forcedCategory = "회의록";
            }
            // 3. 일정 공식 (3/12 같은 슬래시 날짜 포맷도 감지하도록 정규식 추가!)
            else if ((txt.includes("회의") || txt.includes("미팅") || txt.includes("약속") || txt.includes("내일") || txt.includes("일정") || /\d+월\s*\d+일/.test(txt) || /\d+\/\d+/.test(txt)) && !forcedCategory) {
                forcedCategory = "일정";
            }

            if (forcedCategory && categories.includes(forcedCategory)) {
                self.postMessage({ type: "TOKEN", text: " (규칙 기반 초고속 분류 완료 ⚡)" });
                
                let totalAmount = 0;
                if (forcedCategory === "지출") {
                    const matches = txt.match(/\b\d{1,3}(?:,\d{3})*\b|\b\d+\b/g);
                    if (matches) {
                        matches.forEach(m => {
                            const num = parseInt(m.replace(/,/g, ''), 10);
                            if (!isNaN(num) && num >= 100) { totalAmount += num; }
                        });
                    }
                }

                const finalResult = {
                    category: forcedCategory,
                    content: txt.replace(/\n/g, "  \n"),
                    amount: forcedCategory === "지출" ? totalAmount : null
                };
                
                self.postMessage({ type: "DONE_CLASSIFY", result: JSON.stringify(finalResult) });
                return; 
            }

            // [2단계] AI 판단 영역
            const categoriesStr = categories.join(", ");
            const systemPrompt = `You are a categorization bot. Classify the text into one of: [${categoriesStr}, 미분류].
- If it's about spending money or prices, choose "지출".
- If it's a meeting result, choose "회의록".
- If it's a future schedule, choose "일정".
Output ONLY JSON in format {"category": "..."}`;

            const chunks = await llmEngine.chat.completions.create({
                messages: [
                    { role: "system", content: systemPrompt },
                    { role: "user", content: txt }
                ],
                temperature: 0.0, 
                stream: true,
            });

            let reply = "";
            for await (const chunk of chunks) {
                const text = chunk.choices[0]?.delta?.content || "";
                reply += text;
                self.postMessage({ type: "TOKEN", text });
            }

            try {
                const jsonMatch = reply.match(/\{[\s\S]*\}/);
                if (jsonMatch) {
                    const cleanJson = jsonMatch[0];
                    const obj = JSON.parse(cleanJson);
                    obj.content = txt.replace(/\n/g, "  \n");
                    
                    if (!categories.includes(obj.category)) {
                        obj.category = "미분류";
                    }
                    self.postMessage({ type: "DONE_CLASSIFY", result: JSON.stringify(obj) });
                } else {
                    throw new Error("응답에서 JSON 구조를 찾을 수 없습니다.");
                }
            } catch (e) {
                const fallbackObj = {
                    category: "미분류",
                    content: txt.replace(/\n/g, "  \n")
                };
                self.postMessage({ type: "DONE_CLASSIFY", result: JSON.stringify(fallbackObj) });
            }
        }
        // ... (이하 VECTOR_SEARCH 코드는 동일합니다)
        else if (command === "VECTOR_SEARCH") {
            const payload = JSON.parse(data.text);
            const query = payload.query;
            const memos = payload.memos;

            if (!memos || memos.length === 0) {
                self.postMessage({ type: "DONE_SEARCH", result: JSON.stringify([]) });
                return;
            }

            self.postMessage({ type: "TOKEN_SEARCH", text: `스마트 하이브리드 정밀 탐색 중...` });

            const memoTexts = memos.map(m => m.text);
            const [queryOut, memosOut] = await Promise.all([
                embedder(query, { pooling: 'mean', normalize: true }),
                embedder(memoTexts, { pooling: 'mean', normalize: true })
            ]);

            const queryVector = Array.from(queryOut.data);
            const results = [];
            const dimension = queryVector.length;

            for (let i = 0; i < memos.length; i++) {
                const memoText = memos[i].text;
                const memoVector = Array.from(memosOut.data.slice(i * dimension, (i + 1) * dimension));
                
                const semanticScore = cosineSimilarity(queryVector, memoVector);
                const keywordBoost = calculateKoreanHybridBoost(query, memoText);

                results.push({ id: memos[i].id, score: semanticScore + keywordBoost });
            }

            results.sort((a, b) => b.score - a.score);

            if (results.length > 0) {
                const bestScore = results[0].score;
                const filteredResults = results.filter(r => r.score >= 0.45 && r.score >= bestScore - 0.15);
                const topIds = filteredResults.slice(0, 5).map(r => r.id);
                
                self.postMessage({ type: "DONE_SEARCH", result: JSON.stringify(topIds) });
            } else {
                self.postMessage({ type: "DONE_SEARCH", result: JSON.stringify([]) });
            }
        }
    } catch (err) {
        console.error("🚨 [Worker] OnMessage 처리 중 에러 발생:", err);
        self.postMessage({ type: "ERROR", text: err.toString() });
    }
};