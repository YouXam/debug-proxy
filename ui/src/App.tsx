import { useState, useEffect } from 'react'
import { 
  Settings, 
  Activity, 
  Trash2, 
  RefreshCw, 
  Server, 
  Globe, 
  Clock,
  Database,
  Scissors,
  CheckCircle,
  XCircle
} from 'lucide-react'
import './App.css'

interface Config {
  client_timeout_ms: number;
  upstream_timeout_ms: number;
  max_history_size: number;
  max_body_size: number;
  truncate_body_at: number;
}

interface HttpTransaction {
  request: {
    id: string;
    timestamp: number;
    method: string;
    path: string;
    version: string;
    headers: [string, string][];
    body: {
      content_type: string;
      is_binary: boolean;
      size: number;
      preview: string;
    };
    client_addr: string;
  };
  response?: {
    id: string;
    timestamp: number;
    status: number;
    version: string;
    headers: [string, string][];
    body: {
      content_type: string;
      is_binary: boolean;
      size: number;
      preview: string;
    };
    duration_ms: number;
  };
  error?: string;
}

function App() {
  const [config, setConfig] = useState<Config | null>(null);
  const [transactions, setTransactions] = useState<HttpTransaction[]>([]);
  const [loading, setLoading] = useState(true);
  const [notification, setNotification] = useState<{ message: string; type: 'success' | 'error' } | null>(null);
  const [refreshing, setRefreshing] = useState(false);

  // Get token from URL params
  const urlParams = new URLSearchParams(window.location.search);
  const token = urlParams.get('token') || '';

  // Auto-refresh transactions every 5 seconds
  useEffect(() => {
    const interval = setInterval(() => {
      if (!refreshing) {
        fetchTransactions();
      }
    }, 5000);
    return () => clearInterval(interval);
  }, [refreshing]);

  // Initial load
  useEffect(() => {
    Promise.all([fetchConfig(), fetchTransactions()]).finally(() => {
      setLoading(false);
    });
  }, []);

  const showNotification = (message: string, type: 'success' | 'error' = 'success') => {
    setNotification({ message, type });
    setTimeout(() => setNotification(null), 3000);
  };

  const fetchConfig = async () => {
    try {
      const response = await fetch(`/_proxy/api/config?token=${token}`);
      if (response.ok) {
        const data = await response.json();
        setConfig(data);
      } else {
        throw new Error('Failed to fetch config');
      }
    } catch (error) {
      console.error('Error fetching config:', error);
      showNotification('Failed to load configuration', 'error');
    }
  };

  const fetchTransactions = async () => {
    try {
      const response = await fetch(`/_proxy/api/logs?token=${token}`);
      if (response.ok) {
        const data = await response.json();
        setTransactions(data);
      } else {
        throw new Error('Failed to fetch transactions');
      }
    } catch (error) {
      console.error('Error fetching transactions:', error);
    }
  };

  const updateConfig = async (newConfig: Partial<Config>) => {
    try {
      const response = await fetch(`/_proxy/api/config?token=${token}`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(newConfig),
      });
      
      if (response.ok) {
        await fetchConfig();
        showNotification('Configuration updated successfully');
      } else {
        throw new Error('Failed to update config');
      }
    } catch (error) {
      console.error('Error updating config:', error);
      showNotification('Failed to update configuration', 'error');
    }
  };

  const clearLogs = async () => {
    setRefreshing(true);
    try {
      const response = await fetch(`/_proxy/api/logs?token=${token}`, {
        method: 'DELETE',
      });
      
      if (response.ok) {
        setTransactions([]);
        showNotification('Request logs cleared');
      } else {
        throw new Error('Failed to clear logs');
      }
    } catch (error) {
      console.error('Error clearing logs:', error);
      showNotification('Failed to clear logs', 'error');
    } finally {
      setRefreshing(false);
    }
  };

  const manualRefresh = async () => {
    setRefreshing(true);
    try {
      await fetchTransactions();
      showNotification('Logs refreshed');
    } catch (error) {
      showNotification('Failed to refresh logs', 'error');
    } finally {
      setRefreshing(false);
    }
  };

  if (loading) {
    return (
      <div className="loading">
        <RefreshCw className="loading-spinner" />
        <p>Loading DebugProxy interface...</p>
      </div>
    );
  }

  return (
    <div className="debug-proxy">
      <header className="header">
        <div className="header-content">
          <div className="title-section">
            <Globe className="title-icon" />
            <h1>DebugProxy</h1>
            <span className="subtitle">HTTP Debugging Reverse Proxy</span>
          </div>
        </div>
      </header>

      {notification && (
        <div className={`notification notification-${notification.type}`}>
          {notification.type === 'success' ? <CheckCircle size={16} /> : <XCircle size={16} />}
          {notification.message}
        </div>
      )}

      <main className="main-content">
        <div className="config-section">
          <div className="section-header">
            <Settings size={20} />
            <h2>Configuration</h2>
          </div>
          
          {config && (
            <div className="config-grid">
              <div className="config-item">
                <Clock size={16} />
                <label>Client Timeout (ms)</label>
                <input
                  type="number"
                  value={config.client_timeout_ms}
                  onChange={(e) => updateConfig({ client_timeout_ms: parseInt(e.target.value) })}
                />
              </div>
              
              <div className="config-item">
                <Server size={16} />
                <label>Upstream Timeout (ms)</label>
                <input
                  type="number"
                  value={config.upstream_timeout_ms}
                  onChange={(e) => updateConfig({ upstream_timeout_ms: parseInt(e.target.value) })}
                />
              </div>
              
              <div className="config-item">
                <Database size={16} />
                <label>Max History Size</label>
                <input
                  type="number"
                  value={config.max_history_size}
                  onChange={(e) => updateConfig({ max_history_size: parseInt(e.target.value) })}
                />
              </div>
              
              <div className="config-item">
                <Scissors size={16} />
                <label>Body Truncation (bytes)</label>
                <input
                  type="number"
                  value={config.truncate_body_at}
                  onChange={(e) => updateConfig({ truncate_body_at: parseInt(e.target.value) })}
                />
              </div>
            </div>
          )}
        </div>

        <div className="logs-section">
          <div className="section-header">
            <Activity size={20} />
            <h2>Request Logs</h2>
            <div className="log-controls">
              <button
                className="control-button"
                onClick={manualRefresh}
                disabled={refreshing}
                title="Refresh logs"
              >
                <RefreshCw className={refreshing ? 'spinning' : ''} size={16} />
              </button>
              <button
                className="control-button danger"
                onClick={clearLogs}
                disabled={refreshing}
                title="Clear all logs"
              >
                <Trash2 size={16} />
              </button>
            </div>
          </div>

          <div className="transactions-list">
            {transactions.length === 0 ? (
              <div className="empty-state">
                <Activity size={48} />
                <p>No requests recorded yet</p>
                <small>Requests will appear here as they are proxied</small>
              </div>
            ) : (
              transactions.map((transaction) => (
                <TransactionCard key={transaction.request.id} transaction={transaction} />
              ))
            )}
          </div>
        </div>
      </main>
    </div>
  );
}

function TransactionCard({ transaction }: { transaction: HttpTransaction }) {
  const [expanded, setExpanded] = useState(false);
  
  const formatTimestamp = (timestamp: string | number) => {
    try {
      // Handle both string ISO dates and numeric timestamps
      const date = typeof timestamp === 'number' ? new Date(timestamp) : new Date(timestamp);
      if (isNaN(date.getTime())) {
        return 'Invalid Date';
      }
      return date.toLocaleTimeString();
    } catch (e) {
      return 'Invalid Date';
    }
  };

  const getStatusColor = (statusCode?: number) => {
    if (!statusCode) return 'pending';
    if (statusCode >= 200 && statusCode < 300) return 'success';
    if (statusCode >= 300 && statusCode < 400) return 'warning';
    if (statusCode >= 400) return 'error';
    return 'info';
  };

  return (
    <div className={`transaction-card ${expanded ? 'expanded' : ''}`}>
      <div className="transaction-header" onClick={() => setExpanded(!expanded)}>
        <div className="transaction-basic">
          <span className={`method method-${transaction.request.method.toLowerCase()}`}>
            {transaction.request.method}
          </span>
          <span className="path">{transaction.request.path}</span>
          <span className="timestamp">{formatTimestamp(transaction.request.timestamp)}</span>
        </div>
        
        <div className="transaction-status">
          {transaction.error ? (
            <span className="status error">ERROR</span>
          ) : transaction.response ? (
            <span className={`status ${getStatusColor(transaction.response.status)}`}>
              {transaction.response.status}
            </span>
          ) : (
            <span className="status pending">PENDING</span>
          )}
          {transaction.response && (
            <span className="duration">{transaction.response.duration_ms}ms</span>
          )}
        </div>
      </div>

      {expanded && (
        <div className="transaction-details">
          <div className="detail-section">
            <h4>Request Headers</h4>
            <div className="headers">
              {transaction.request.headers.map(([name, value], index) => (
                <div key={`${name}-${index}`} className="header-item">
                  <span className="header-name">{name}:</span>
                  <span className="header-value">{value}</span>
                </div>
              ))}
            </div>
          </div>

          {transaction.request.body.size > 0 && (
            <div className="detail-section">
              <h4>Request Body ({transaction.request.body.size} bytes)</h4>
              {transaction.request.body.is_binary ? (
                <div className="binary-body">Binary content ({transaction.request.body.size} bytes)</div>
              ) : (
                <pre className="body-content">{transaction.request.body.preview}</pre>
              )}
            </div>
          )}

          {transaction.response && (
            <>
              <div className="detail-section">
                <h4>Response Headers</h4>
                <div className="headers">
                  {transaction.response.headers.map(([name, value], index) => (
                    <div key={`${name}-${index}`} className="header-item">
                      <span className="header-name">{name}:</span>
                      <span className="header-value">{value}</span>
                    </div>
                  ))}
                </div>
              </div>

              {transaction.response.body.size > 0 && (
                <div className="detail-section">
                  <h4>Response Body ({transaction.response.body.size} bytes)</h4>
                  {transaction.response.body.is_binary ? (
                    <div className="binary-body">Binary content ({transaction.response.body.size} bytes)</div>
                  ) : (
                    <pre className="body-content">{transaction.response.body.preview}</pre>
                  )}
                </div>
              )}
            </>
          )}

          {transaction.error && (
            <div className="detail-section">
              <h4>Error</h4>
              <div className="error-message">{transaction.error}</div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export default App