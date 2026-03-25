/// Minimal 2D tensor for ML demos. No framework — just Vec<f32>.
/// In production you'd use PyTorch/ndarray. This shows the raw math.

#[derive(Debug, Clone)]
pub struct Tensor {
    pub data: Vec<f32>,
    pub rows: usize,
    pub cols: usize,
}

impl Tensor {
    pub fn zeros(rows: usize, cols: usize) -> Self {
        Self { data: vec![0.0; rows * cols], rows, cols }
    }

    pub fn from_vec(data: Vec<f32>, rows: usize, cols: usize) -> Self {
        assert_eq!(data.len(), rows * cols);
        Self { data, rows, cols }
    }

    pub fn rand(rows: usize, cols: usize) -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let data: Vec<f32> = (0..rows * cols).map(|_| rng.gen_range(-1.0..1.0)).collect();
        Self { data, rows, cols }
    }

    pub fn rand_normal(rows: usize, cols: usize, std: f32) -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        // Box-Muller transform for normal distribution
        let data: Vec<f32> = (0..rows * cols).map(|_| {
            let u1: f32 = rng.gen_range(0.001..1.0);
            let u2: f32 = rng.gen_range(0.0..std::f32::consts::TAU);
            (-2.0 * u1.ln()).sqrt() * u2.cos() * std
        }).collect();
        Self { data, rows, cols }
    }

    #[inline]
    pub fn get(&self, r: usize, c: usize) -> f32 {
        self.data[r * self.cols + c]
    }

    #[inline]
    pub fn set(&mut self, r: usize, c: usize, v: f32) {
        self.data[r * self.cols + c] = v;
    }

    /// Matrix multiply: (M×K) × (K×N) → (M×N)
    pub fn matmul(&self, other: &Tensor) -> Tensor {
        assert_eq!(self.cols, other.rows, "matmul dimension mismatch");
        let mut result = Tensor::zeros(self.rows, other.cols);
        for i in 0..self.rows {
            for j in 0..other.cols {
                let mut sum = 0.0;
                for k in 0..self.cols {
                    sum += self.get(i, k) * other.get(k, j);
                }
                result.set(i, j, sum);
            }
        }
        result
    }

    /// Transpose
    pub fn t(&self) -> Tensor {
        let mut result = Tensor::zeros(self.cols, self.rows);
        for i in 0..self.rows {
            for j in 0..self.cols {
                result.set(j, i, self.get(i, j));
            }
        }
        result
    }

    /// Element-wise add
    pub fn add(&self, other: &Tensor) -> Tensor {
        assert_eq!(self.data.len(), other.data.len());
        let data: Vec<f32> = self.data.iter().zip(&other.data).map(|(a, b)| a + b).collect();
        Tensor { data, rows: self.rows, cols: self.cols }
    }

    /// Add bias (broadcast row vector across all rows)
    pub fn add_bias(&self, bias: &[f32]) -> Tensor {
        assert_eq!(bias.len(), self.cols);
        let mut result = self.clone();
        for i in 0..self.rows {
            for j in 0..self.cols {
                result.data[i * self.cols + j] += bias[j];
            }
        }
        result
    }

    /// Element-wise multiply
    pub fn mul(&self, other: &Tensor) -> Tensor {
        let data: Vec<f32> = self.data.iter().zip(&other.data).map(|(a, b)| a * b).collect();
        Tensor { data, rows: self.rows, cols: self.cols }
    }

    /// Scalar multiply
    pub fn scale(&self, s: f32) -> Tensor {
        let data: Vec<f32> = self.data.iter().map(|x| x * s).collect();
        Tensor { data, rows: self.rows, cols: self.cols }
    }

    /// Get a row as a slice
    pub fn row(&self, r: usize) -> &[f32] {
        &self.data[r * self.cols..(r + 1) * self.cols]
    }

    /// Print first few values
    pub fn preview(&self) -> String {
        let vals: Vec<String> = self.data.iter().take(6).map(|v| format!("{:.3}", v)).collect();
        let suffix = if self.data.len() > 6 { ", ..." } else { "" };
        format!("Tensor({}×{}) [{}{}]", self.rows, self.cols, vals.join(", "), suffix)
    }
}
