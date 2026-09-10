use repotrim_engine::{count_tokens, estimate_tokens, estimate_tokens_calibrated, TokenizerModel};

/// Polyglot code snippet sample with language annotation and description.
struct SampleSnippet {
    language: &'static str,
    description: &'static str,
    code: &'static str,
}

fn sample_corpus() -> Vec<SampleSnippet> {
    vec![
        // --- Rust Samples ---
        SampleSnippet {
            language: "Rust",
            description: "Simple function signature",
            code: "pub fn calculate_hash(data: &[u8]) -> u64",
        },
        SampleSnippet {
            language: "Rust",
            description: "Generic trait and method signature",
            code: "pub trait GraphStorage<N, E>: Send + Sync where N: Copy + Eq + Hash, E: Clone {\n    fn add_edge(&mut self, source: N, target: N, weight: f32) -> Result<(), StorageError>;\n}",
        },
        SampleSnippet {
            language: "Rust",
            description: "Full function with pattern matching and error handling",
            code: r#"pub fn parse_header(line: &str) -> Result<Header, ParseError> {
    let parts: Vec<&str> = line.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(ParseError::MalformedHeader);
    }
    let key = parts[0].trim().to_ascii_lowercase();
    let value = parts[1].trim().to_string();
    Ok(Header { key, value })
}"#,
        },
        SampleSnippet {
            language: "Rust",
            description: "Struct definition with derive attributes and docstring",
            code: r#"/// Represents a node in the multiplex code connectivity graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymbolNode {
    pub id: SymbolId,
    pub name: String,
    pub kind: SymbolKind,
    pub file_path: PathBuf,
    pub span: TextSpan,
    pub token_cost: usize,
}"#,
        },
        SampleSnippet {
            language: "Rust",
            description: "Async stream processing with tokio select",
            code: r#"pub async fn process_events(mut rx: Receiver<Event>, hub: Arc<EventHub>) {
    while let Some(event) = rx.recv().await {
        match event {
            Event::Refresh { target } => hub.invalidate(&target).await,
            Event::Shutdown => break,
        }
    }
}"#,
        },

        // --- Python Samples ---
        SampleSnippet {
            language: "Python",
            description: "Simple function with type annotations",
            code: "def compute_metrics(predictions: list[float], ground_truth: list[float]) -> dict[str, float]:\n    pass",
        },
        SampleSnippet {
            language: "Python",
            description: "Class with dataclass decorator and methods",
            code: r#"@dataclass(frozen=True)
class ExecutionConfig:
    """Configuration parameters for distributed worker nodes."""
    batch_size: int = 128
    learning_rate: float = 1e-4
    max_epochs: int = 50
    device: str = "cuda"

    def is_gpu_enabled(self) -> bool:
        return self.device.startswith("cuda")"#,
        },
        SampleSnippet {
            language: "Python",
            description: "Function with list comprehension and dictionary comprehension",
            code: r#"def filter_high_scoring_items(scores: dict[str, float], threshold: float = 0.75) -> dict[str, float]:
    """Filters items exceeding relevance threshold and normalizes ranks."""
    valid = {k: v for k, v in scores.items() if v >= threshold}
    total = sum(valid.values()) or 1.0
    return {k: round(v / total, 4) for k, v in valid.items()}"#,
        },
        SampleSnippet {
            language: "Python",
            description: "Async context manager and generator",
            code: r#"async def stream_records(client: AsyncDatabaseClient, query: str):
    async with client.acquire_connection() as conn:
        cursor = await conn.execute(query)
        async for row in cursor:
            yield transform_row(row)"#,
        },

        // --- TypeScript Samples ---
        SampleSnippet {
            language: "TypeScript",
            description: "Generic interface definition with union types",
            code: r#"export interface ApiResponse<T> {
    status: "success" | "error" | "pending";
    statusCode: number;
    data?: T;
    error?: {
        code: string;
        message: string;
        details?: Record<string, unknown>;
    };
}"#,
        },
        SampleSnippet {
            language: "TypeScript",
            description: "Async arrow function with destructuring",
            code: r#"export const fetchUserProfile = async (
    userId: string,
    options: { timeoutMs?: number; includeRoles?: boolean } = {}
): Promise<UserProfile> => {
    const { timeoutMs = 5000, includeRoles = false } = options;
    const response = await fetch(`/api/users/${userId}?roles=${includeRoles}`, {
        signal: AbortSignal.timeout(timeoutMs),
    });
    if (!response.ok) {
        throw new ApiError(`Failed to fetch user ${userId}: ${response.statusText}`);
    }
    return response.json();
};"#,
        },
        SampleSnippet {
            language: "TypeScript",
            description: "React functional component with hooks",
            code: r#"export const ContextMetricsCard: React.FC<MetricsProps> = ({ report, onRefresh }) => {
    const [isLoading, setIsLoading] = useState(false);
    const tokenRatio = useMemo(() => {
        return (report.tokensUsed / report.budget) * 100;
    }, [report.tokensUsed, report.budget]);

    return (
        <div className="card p-4 shadow-sm border">
            <h4 className="font-semibold text-lg">{report.modelName}</h4>
            <div className="progress-bar my-2" style={{ width: `${tokenRatio}%` }} />
            <button onClick={onRefresh} disabled={isLoading} className="btn-primary">
                Refresh Analysis
            </button>
        </div>
    );
};"#,
        },

        // --- Go Samples ---
        SampleSnippet {
            language: "Go",
            description: "Struct definition with json and yaml tags",
            code: r#"type ServerConfig struct {
    Host         string        `json:"host" yaml:"host"`
    Port         int           `json:"port" yaml:"port"`
    ReadTimeout  time.Duration `json:"read_timeout" yaml:"readTimeout"`
    WriteTimeout time.Duration `json:"write_timeout" yaml:"writeTimeout"`
    MaxPayload   int64         `json:"max_payload" yaml:"maxPayload"`
}"#,
        },
        SampleSnippet {
            language: "Go",
            description: "Receiver method with error handling and context",
            code: r#"func (s *IndexServer) QuerySymbols(ctx context.Context, req *QueryRequest) (*QueryResponse, error) {
    if req == nil || req.Query == "" {
        return nil, errors.New("empty query supplied")
    }
    results, err := s.engine.Search(ctx, req.Query, req.Limit)
    if err != nil {
        return nil, fmt.Errorf("engine search failed: %w", err)
    }
    return &QueryResponse{Matches: results, Count: len(results)}, nil
}"#,
        },
        SampleSnippet {
            language: "Go",
            description: "Goroutine worker pool with channel synchronization",
            code: r#"func RunWorkerPool(ctx context.Context, jobs <-chan Job, results chan<- Result, workers int) {
    var wg sync.WaitGroup
    for i := 0; i < workers; i++ {
        wg.Add(1)
        go func(workerID int) {
            defer wg.Done()
            for {
                select {
                case <-ctx.Done():
                    return
                case job, ok := <-jobs:
                    if !ok {
                        return
                    }
                    results <- job.Execute(workerID)
                }
            }
        }(i)
    }
    wg.Wait()
}"#,
        },
    ]
}

/// Computes Pearson correlation coefficient $r$ between two paired series.
fn pearson_correlation(x: &[f64], y: &[f64]) -> f64 {
    assert_eq!(x.len(), y.len());
    let n = x.len() as f64;
    if n < 2.0 {
        return 1.0;
    }

    let mean_x = x.iter().sum::<f64>() / n;
    let mean_y = y.iter().sum::<f64>() / n;

    let mut cov = 0.0;
    let mut var_x = 0.0;
    let mut var_y = 0.0;

    for (&xi, &yi) in x.iter().zip(y.iter()) {
        let dx = xi - mean_x;
        let dy = yi - mean_y;
        cov += dx * dy;
        var_x += dx * dx;
        var_y += dy * dy;
    }

    if var_x == 0.0 || var_y == 0.0 {
        return 0.0;
    }

    cov / (var_x.sqrt() * var_y.sqrt())
}

/// Computes Mean Absolute Percentage Error (MAPE) relative to ground truth $y$.
fn mean_absolute_percentage_error(predicted: &[f64], ground_truth: &[f64]) -> f64 {
    assert_eq!(predicted.len(), ground_truth.len());
    let mut sum_ape = 0.0;
    let mut count = 0;

    for (&p, &g) in predicted.iter().zip(ground_truth.iter()) {
        if g > 0.0 {
            sum_ape += ((p - g) / g).abs();
            count += 1;
        }
    }

    if count == 0 {
        0.0
    } else {
        (sum_ape / count as f64) * 100.0
    }
}

/// Computes 95% bootstrap confidence interval of the mean error $(p - g) / g$.
fn bootstrap_95_ci(predicted: &[f64], ground_truth: &[f64], iterations: usize) -> (f64, f64) {
    let n = predicted.len();
    let errors: Vec<f64> = predicted
        .iter()
        .zip(ground_truth.iter())
        .map(|(&p, &g)| (p - g) / g)
        .collect();

    let mut bootstrap_means = Vec::with_capacity(iterations);
    let mut lcg_seed = 424242u64;

    for _ in 0..iterations {
        let mut sample_sum = 0.0;
        for _ in 0..n {
            lcg_seed = lcg_seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let idx = (lcg_seed >> 32) as usize % n;
            sample_sum += errors[idx];
        }
        bootstrap_means.push(sample_sum / n as f64);
    }

    bootstrap_means.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let lower_idx = (iterations as f64 * 0.025) as usize;
    let upper_idx = (iterations as f64 * 0.975) as usize;

    (bootstrap_means[lower_idx], bootstrap_means[upper_idx])
}

#[test]
fn test_polyglot_token_calibration_study() {
    let corpus = sample_corpus();
    assert!(corpus.len() >= 12);

    let mut ground_truth_cl100k: Vec<f64> = Vec::new();
    let mut ground_truth_o200k: Vec<f64> = Vec::new();
    let mut raw_heuristic_counts: Vec<f64> = Vec::new();
    let mut calibrated_heuristic_counts: Vec<f64> = Vec::new();

    println!("\n=== Polyglot Token Calibration Evaluation ===");
    println!(
        "{:<12} | {:<28} | {:>8} | {:>8} | {:>8} | {:>8}",
        "Lang", "Description", "Raw", "Calib", "cl100k", "o200k"
    );
    println!("{:-<92}", "");

    for sample in &corpus {
        let raw = estimate_tokens(sample.code) as f64;
        let calib = estimate_tokens_calibrated(sample.code) as f64;
        let cl100k = count_tokens(sample.code, TokenizerModel::Cl100kBase) as f64;
        let o200k = count_tokens(sample.code, TokenizerModel::O200kBase) as f64;

        raw_heuristic_counts.push(raw);
        calibrated_heuristic_counts.push(calib);
        ground_truth_cl100k.push(cl100k);
        ground_truth_o200k.push(o200k);

        println!(
            "{:<12} | {:<28} | {:>8.0} | {:>8.0} | {:>8.0} | {:>8.0}",
            sample.language, sample.description, raw, calib, cl100k, o200k
        );
    }

    // 1. Pearson Correlation Analysis
    let r_raw = pearson_correlation(&raw_heuristic_counts, &ground_truth_cl100k);
    let r_calib = pearson_correlation(&calibrated_heuristic_counts, &ground_truth_cl100k);
    let r_o200k = pearson_correlation(&calibrated_heuristic_counts, &ground_truth_o200k);

    println!("\n--- Statistical Correlation Analysis ---");
    println!(
        "Pearson r (Raw Heuristic vs cl100k_base):         {:.4}",
        r_raw
    );
    println!(
        "Pearson r (Calibrated Heuristic vs cl100k_base):  {:.4}",
        r_calib
    );
    println!(
        "Pearson r (Calibrated Heuristic vs o200k_base):   {:.4}",
        r_o200k
    );
    println!(
        "Coefficient of Determination R^2 (cl100k):       {:.4}",
        r_calib * r_calib
    );

    // Pearson r must demonstrate strong linear alignment (> 0.90)
    assert!(
        r_calib > 0.90,
        "Expected Pearson r > 0.90, got {:.4}",
        r_calib
    );

    // 2. Mean Absolute Percentage Error (MAPE)
    let mape_raw = mean_absolute_percentage_error(&raw_heuristic_counts, &ground_truth_cl100k);
    let mape_calib =
        mean_absolute_percentage_error(&calibrated_heuristic_counts, &ground_truth_cl100k);

    println!("\n--- Mean Absolute Percentage Error (MAPE) ---");
    println!("MAPE (Raw Heuristic):         {:.2}%", mape_raw);
    println!("MAPE (Calibrated Heuristic):  {:.2}%", mape_calib);

    // Calibration must reduce error or maintain competitive accuracy (< 18%)
    assert!(
        mape_calib < 18.0,
        "Expected MAPE < 18%, got {:.2}%",
        mape_calib
    );

    // 3. 95% Bootstrap Confidence Interval
    let (ci_lower, ci_upper) =
        bootstrap_95_ci(&calibrated_heuristic_counts, &ground_truth_cl100k, 2000);
    println!("\n--- 95% Bootstrap Confidence Interval for Relative Error ---");
    println!(
        "95% CI: [{:+.2}%, {:+.2}%]",
        ci_lower * 100.0,
        ci_upper * 100.0
    );

    // 4. Exact BPE Consistency Checks
    #[cfg(feature = "exact-tokens")]
    {
        for sample in &corpus {
            let cl = count_tokens(sample.code, TokenizerModel::Cl100kBase);
            let o2 = count_tokens(sample.code, TokenizerModel::O200kBase);
            assert!(cl > 0, "Exact cl100k count must be > 0");
            assert!(o2 > 0, "Exact o200k count must be > 0");
            // o200k vocabulary is larger, so token counts are typically <= cl100k
            assert!(
                (o2 as f64) <= (cl as f64) * 1.25,
                "o200k ({}) should be comparable to cl100k ({})",
                o2,
                cl
            );
        }
    }
}
