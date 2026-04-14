FROM shancw/perplexity-mcp:latest

# 补丁：允许空 token 池启动（原版要求至少一个 token，否则崩溃）
# 同时设置 _config_path，保证通过 API 添加的账号能持久化到配置文件
RUN python3 -c "\
content = open('/app/perplexity/server/client_pool.py').read(); \
content = content.replace(\
    'raise ValueError(f\"No tokens found in config file: {config_path}\")', \
    'logger.warning(f\"Empty token pool from {config_path}, will add via API\"); self._config_path = config_path; return'\
); \
open('/app/perplexity/server/client_pool.py', 'w').write(content)"
