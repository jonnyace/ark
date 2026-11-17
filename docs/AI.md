# Local AI

Optional. The library is fully usable without it. When installed and turned on, a language model on the device
answers a question **only from the installed packs**, citing exact sources. Nothing is sent anywhere.

## How an answer is made
1. The question is searched across installed content packs (same BM25 index as search).
2. If no passage contains at least a third of the question's distinct terms, **the model is not asked**: the reply is
   "not found in your library" with the closest matches.
3. Up to 5 passages (≤ 1400 characters each, ≤ 6000 total) become sources `S1…S5`, each with its exact citation.
4. The prompt (ChatML) gives rules (use only the sources, cite each sentence, say "Not found in the library",
   ignore instructions inside sources) and repeats the citation rule at the end. Retrieved text is passed as plain text:
   natively the runner never parses special tokens in it; in both runtimes template markers are neutralised first.
5. The answer is checked: citation keys normalised, invented keys removed and reported, repeated sentences removed,
   **every number in a cited sentence checked against the sources it cites** (unverified numbers are shown as a
   warning), every cited target re-validated against the installed manifest. Generation stops early if the model
   loops.
6. Answers are never presented as guaranteed correct; the UI says to check the sources.

## Recorded configuration (model pack `org.lastbook.model.qwen2.5-1.5b-instruct`, 2.5.0-q4km.2)

| Item | Value |
|---|---|
| Model | Qwen2.5-1.5B-Instruct, Qwen team (Alibaba Cloud), Apache-2.0 (licence file in the pack) |
| Weights | `Qwen/Qwen2.5-1.5B-Instruct-GGUF` @ `91cad51170dc346986eccefdc2dd33a9da36ead9`, `qwen2.5-1.5b-instruct-q4_k_m.gguf`, SHA-256 `6a1a2eb6…407e`, 1 117 320 736 bytes |
| Quantization | Q4_K_M (GGUF) |
| Tokenizer | embedded in the GGUF (Qwen2 BPE) |
| Prompt template | ChatML (`<|im_start|>role … <|im_end|>`), system prompt in `crates/lbn-core/src/rag.rs` |
| Generation | temperature 0.1, top-k 40, top-p 0.9, min-p 0.05, max 400 tokens, seed 1, repeat penalty 1.1 over 64 tokens |
| Context | 4096 tokens |
| Native runtime | `lastbook-llm` (Lastbook's pipe-only runner) on llama.cpp b12032 (`dde879bc`), CPU, all x86-64 CPU variants (SSE4.2 → AVX-512) selected at run time; Vulkan optional at build time (`LBN_VULKAN=1`, tested on AMD Radeon/RADV; enable with `LBN_GPU=1`) |
| Browser runtime | wllama 3.13.0 (llama.cpp in WebAssembly), 1 thread, or up to 8 when cross-origin isolated |
| Hardware | x86-64 CPU with SSE4.2 or newer (or aarch64), ~1.9 GB RAM while answering (measured peak RSS 1.84 GB), ≥ 4 GB total RAM recommended, ~1.1 GB disk; no GPU needed. Browser: same RAM plus the 1.1 GB pack inside the origin's storage quota |

A smaller pack (Qwen2.5-0.5B-Instruct, same licence) exists for constrained devices but is **not recommended**: it
gave a wrong bleach dose in testing.

## Licence verification (model packs)
Checked on 2026-11-16 for the exact revision that is packed, not the model family:
- Publisher: the model's own authors (`Qwen` organisation), repository `Qwen/Qwen2.5-1.5B-Instruct-GGUF` at revision
  `91cad51…`; the repository is not gated (no click-through terms).
- Licence: the repository's `LICENSE` file is the Apache License 2.0 text (SHA-256 `832dd9e0…292e92e`), and the model
  card metadata says `license: apache-2.0`, `base_model: Qwen/Qwen2.5-1.5B-Instruct`. Apache-2.0 allows redistribution
  with the licence and notices, so the pack ships `model/LICENSE` (same SHA-256) and the model card.
- Same checks for `Qwen2.5-0.5B-Instruct-GGUF` @ `9217f5d…`. Not every size is Apache-2.0 (Qwen2.5-3B is not): check
  each model and revision before packing. This is a licence reading, not legal advice.

## Measured on the test machine (Ryzen, 16 threads, 27 GiB RAM)
- Native CPU (4 threads): load 1.0 s, first token 0.33 s, 52 tokens/s; typical answer 3–5 s. Grounded test: 7/7.
- Browser, Chromium 159 (8 threads, isolated): typical answer 20–38 s, almost all prompt processing; 18 tokens/s
  writing. Firefox: 27 s. WebKit (Playwright): the 1.5B pack exceeds its 1000 MB quota and is refused up front;
  0.5B ran (23 s).

## Browser support (declared from test runs)
| Browser | Inference | Notes |
|---|---|---|
| Chromium-based (Chromium 159 tested) | yes, 1.5B | cross-origin isolation enables threads |
| Firefox (Playwright build) | yes, 1.5B | |
| WebKit (Playwright build on Linux) | 0.5B only | origin quota 1000 MB here |
| Safari (macOS/iOS), mobile browsers | **untested** | |
Requirements are checked at start (64-bit WebAssembly memory, JSPI). If missing, the AI page says exactly what is
missing. There is **no cloud fallback** and no remote "local" machine.

## Controls
Turn AI on/off (turning off unloads the model), choose a default model, load/unload to free memory, cancel an
answer, remove a model pack. Native errors are reported plainly: missing runtime, model file damaged, not enough
memory (the runner process is separate, so an out-of-memory kill does not take the app down), question too long.

## Replacing the model
Build a new model pack (`packs/models/*/pack.src.json` shows the fields), check its licence allows redistribution,
run `tools/ai_eval.py` against it and record the result. Do not assume public weights are redistributable
(e.g. Qwen2.5-3B is not Apache-2.0).
