// =============================================================================
// Ring AllReduce — how NCCL averages gradients across GPUs
//
// The algorithm:
//   N GPUs in a ring, each has a gradient vector.
//   Phase 1 (scatter-reduce): N-1 steps, each GPU sends/receives chunks
//     → each GPU ends up with the COMPLETE SUM of one chunk
//   Phase 2 (allgather): N-1 steps, share complete chunks around the ring
//     → every GPU has the full sum (or average)
//
//   Total data sent per GPU: 2 × (N-1)/N × data_size
//   For large N: ≈ 2 × data_size (independent of number of GPUs!)
// =============================================================================

pub fn demo() {
    println!("\n  Ring AllReduce simulation (4 GPUs, gradient size = 8)\n");

    let num_gpus = 4;
    let grad_size = 8;

    // Each GPU starts with its own gradient vector
    let mut gpus: Vec<Vec<f32>> = vec![
        vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],   // GPU 0
        vec![2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0],   // GPU 1
        vec![3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],  // GPU 2
        vec![4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0], // GPU 3
    ];

    println!("    Initial state (each GPU has its own gradients):");
    for (i, gpu) in gpus.iter().enumerate() {
        println!("      GPU {}: {:?}", i, gpu);
    }

    // Expected average
    let expected: Vec<f32> = (0..grad_size).map(|j| {
        gpus.iter().map(|g| g[j]).sum::<f32>() / num_gpus as f32
    }).collect();

    // Chunk size: split gradient into N chunks
    let chunk_size = grad_size / num_gpus;

    // Phase 1: Scatter-Reduce
    // In each step, GPU i sends chunk (i - step) to GPU (i+1),
    // and receives + accumulates from GPU (i-1).
    println!("\n    Phase 1: Scatter-Reduce ({} steps)", num_gpus - 1);

    for step in 0..num_gpus - 1 {
        let mut new_gpus = gpus.clone();
        for gpu_id in 0..num_gpus {
            let send_chunk_idx = (gpu_id + num_gpus - step) % num_gpus;
            let recv_from = (gpu_id + num_gpus - 1) % num_gpus;
            let recv_chunk_idx = (gpu_id + num_gpus - step - 1) % num_gpus;

            // Receive chunk from left neighbor, add to local chunk
            let start = recv_chunk_idx * chunk_size;
            for k in 0..chunk_size {
                new_gpus[gpu_id][start + k] += gpus[recv_from][start + k];
            }
        }
        gpus = new_gpus;

        if step == num_gpus - 2 {
            println!("      After step {}: each GPU has the COMPLETE SUM of one chunk", step + 1);
            for (i, gpu) in gpus.iter().enumerate() {
                let own_chunk = i; // after scatter-reduce, GPU i has the sum of chunk i
                let start = own_chunk * chunk_size;
                println!("        GPU {} owns chunk {} [{:.0}, {:.0}] (sum of all GPUs' values)",
                    i, own_chunk, gpus[i][start], gpus[i][start + 1]);
            }
        }
    }

    // Phase 2: AllGather
    // Each GPU broadcasts its complete chunk to all others.
    println!("\n    Phase 2: AllGather ({} steps)", num_gpus - 1);

    for step in 0..num_gpus - 1 {
        let mut new_gpus = gpus.clone();
        for gpu_id in 0..num_gpus {
            let recv_from = (gpu_id + num_gpus - 1) % num_gpus;
            let recv_chunk_idx = (gpu_id + num_gpus - step - 1) % num_gpus;
            let start = recv_chunk_idx * chunk_size;
            for k in 0..chunk_size {
                new_gpus[gpu_id][start + k] = gpus[recv_from][start + k];
            }
        }
        gpus = new_gpus;
    }

    // Convert sum to average
    for gpu in &mut gpus {
        for v in gpu.iter_mut() {
            *v /= num_gpus as f32;
        }
    }

    println!("\n    Final state (every GPU has the average):");
    for (i, gpu) in gpus.iter().enumerate() {
        println!("      GPU {}: {:?}", i, gpu);
    }
    println!("      Expected: {:?}", expected);

    let correct = gpus.iter().all(|gpu| {
        gpu.iter().zip(&expected).all(|(a, b)| (a - b).abs() < 0.01)
    });
    println!("      All GPUs match: {}\n", if correct { "yes" } else { "NO!" });

    println!("    Key insight: total data per GPU = 2 x (N-1)/N x size");
    println!("    For N=1024 GPUs: still only ~2x gradient size. Scales!\n");
}
