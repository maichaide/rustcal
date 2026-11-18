# 本地测试CalDAV日历同步过程

测试日历客户端的 CalDAV 同步功能，通常不需要专门花钱去买商业托管服务。目前有一些完全免费的方案、公开的测试服务或极低成本的自建方式

**Radicale**：

- 一个用 Python 编写的超级轻量级 CalDAV/CardDAV 服务器。   
- **安装运行**：只需执行 `pip install radicale`，然后运行 `radicale` 即可启动。它默认不需要复杂的数据库配置，直接把数据存在本地文件夹里，非常适合用来给客户端做同步、冲突和增删改查测试。

直接在本地电脑用 `pip install radicale` 跑一个实例。把客户端的服务器地址指向 `http://localhost:5232/`，新建几个用户就能马上开始联调。



## 配置服务并运行

1. 建立服务文件夹，例如：c:/1/radicale

2. 手动建立同步用户文件，例如：`users`内容如下：

   ```
   xyzcal:xyzcal
   ```

   

3. 手动建立`radicale`服务配置文件 `config` 内容如下：

```
[server]
hosts = 0.0.0.0:5232

[auth]
type = htpasswd
htpasswd_filename = C:/1/radicale/users
htpasswd_encryption = plain
```

4. 指定配置文件并启动日历同步服务

```
python -m radicale --config "C:/1/radicale/config"    
```

5. 测试`rustcal`的同步功能

   * 点击 **外观与同步选项卡** ，在 **云端日历同步与协作** 设置`rustcal`的同步协议为`CalDAV`

   * 服务器URL：`http://localhost:5232/xyzcal`

   * 账号：xyzcal

   * 密码：xyzcal

   * 点击 **测试连接**，提示连接成功，再点击 **立即同步所以已启用日历**，顺利完成同步。

     