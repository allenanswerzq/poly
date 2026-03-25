//! ML Systems — mini demos showing how the infrastructure works under the hood.
//!
//! 1. Ring AllReduce (NCCL)
//! 2. ZeRO Sharding (DeepSpeed/FSDP)
//! 3. Flash Attention (tiled + online softmax)
//! 4. PagedAttention (vLLM-style KV cache management)
//! 5. LoRA (low-rank adaptation)
//! 6. INT8 Quantization
//! 7. Sequence Packing (data loading)

mod ring_allreduce;
mod zero_sharding;
mod flash_attn;
mod paged_attn;
mod lora;
mod quantize;
mod packing;

fn main() {
    println!("╔══════════════════════════════════════════════════╗");
    println!("║    ML Systems — Under the Hood Demos             ║");
    println!("╚══════════════════════════════════════════════════╝\n");

    println!("━━━ 1. Ring AllReduce (NCCL) ━━━");
    ring_allreduce::demo();

    println!("━━━ 2. ZeRO Sharding (DeepSpeed/FSDP) ━━━");
    zero_sharding::demo();

    println!("━━━ 3. Flash Attention (tiled + online softmax) ━━━");
    flash_attn::demo();

    println!("━━━ 4. PagedAttention (vLLM KV cache) ━━━");
    paged_attn::demo();

    println!("━━━ 5. LoRA (low-rank adaptation) ━━━");
    lora::demo();

    println!("━━━ 6. INT8 Quantization ━━━");
    quantize::demo();

    println!("━━━ 7. Sequence Packing ━━━");
    packing::demo();

    println!("╔══════════════════════════════════════════════════╗");
    println!("║              Demo Complete!                      ║");
    println!("╚══════════════════════════════════════════════════╝");
}
