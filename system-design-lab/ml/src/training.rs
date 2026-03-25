// =============================================================================
// Training & Optimization
// =============================================================================

// ── #29 Adam Optimizer ───────────────────────────────────────────────────────
// Combines momentum (first moment) + RMSProp (second moment).
// m = β1*m + (1-β1)*grad        — momentum (exponential moving avg of gradients)
// v = β2*v + (1-β2)*grad²       — RMSProp (exponential moving avg of squared gradients)
// m_hat = m / (1 - β1^t)        — bias correction (important early on)
// v_hat = v / (1 - β2^t)
// param -= lr * m_hat / (sqrt(v_hat) + eps)
pub struct Adam {
    lr: f32,
    beta1: f32,
    beta2: f32,
    eps: f32,
    m: Vec<f32>,  // first moment
    v: Vec<f32>,  // second moment
    t: usize,     // step count
}

impl Adam {
    pub fn new(num_params: usize, lr: f32) -> Self {
        Self {
            lr,
            beta1: 0.9,
            beta2: 0.999,
            eps: 1e-8,
            m: vec![0.0; num_params],
            v: vec![0.0; num_params],
            t: 0,
        }
    }

    pub fn step(&mut self, params: &mut [f32], grads: &[f32]) {
        self.t += 1;
        let bc1 = 1.0 - self.beta1.powi(self.t as i32); // bias correction 1
        let bc2 = 1.0 - self.beta2.powi(self.t as i32); // bias correction 2

        for i in 0..params.len() {
            // Update moments
            self.m[i] = self.beta1 * self.m[i] + (1.0 - self.beta1) * grads[i];
            self.v[i] = self.beta2 * self.v[i] + (1.0 - self.beta2) * grads[i] * grads[i];

            // Bias-corrected estimates
            let m_hat = self.m[i] / bc1;
            let v_hat = self.v[i] / bc2;

            // Update parameter
            params[i] -= self.lr * m_hat / (v_hat.sqrt() + self.eps);
        }
    }
}

// ── #30 Cosine LR Scheduler ─────────────────────────────────────────────────
// Linear warmup for first N steps, then cosine decay to min_lr.
//   warmup: lr = base_lr * (step / warmup_steps)
//   decay:  lr = min_lr + 0.5 * (base_lr - min_lr) * (1 + cos(π * progress))
pub fn cosine_lr_schedule(step: usize, total_steps: usize, warmup_steps: usize, base_lr: f32, min_lr: f32) -> f32 {
    if step < warmup_steps {
        // Linear warmup
        base_lr * (step as f32 / warmup_steps as f32)
    } else {
        // Cosine decay
        let progress = (step - warmup_steps) as f32 / (total_steps - warmup_steps) as f32;
        min_lr + 0.5 * (base_lr - min_lr) * (1.0 + (std::f32::consts::PI * progress).cos())
    }
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("\n  ═══ Training & Optimization ═══\n");

    // Adam optimizer
    let mut params = vec![5.0, -3.0, 1.0];
    let grads = vec![1.0, -0.5, 0.2];
    let mut adam = Adam::new(3, 0.001);

    println!("    Adam optimizer (lr=0.001, β1=0.9, β2=0.999):");
    println!("    Initial params: {:?}", params);
    for step in 0..5 {
        adam.step(&mut params, &grads);
    }
    println!("    After 5 steps:  [{:.4}, {:.4}, {:.4}]", params[0], params[1], params[2]);

    // Cosine LR schedule
    println!("\n    Cosine LR schedule (100 steps, 10 warmup, base=0.001, min=0.0001):");
    let steps = [0, 5, 10, 25, 50, 75, 99];
    for &s in &steps {
        let lr = cosine_lr_schedule(s, 100, 10, 0.001, 0.0001);
        let bar_len = (lr / 0.001 * 20.0) as usize;
        let bar: String = "█".repeat(bar_len) + &"░".repeat(20 - bar_len);
        println!("    step {:3}: lr={:.6} [{}]", s, lr, bar);
    }
    println!();
}
