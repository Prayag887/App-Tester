import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';

import '../data/proxy_safety_repository.dart';

class ProxySafetyViewModel extends ChangeNotifier {
  ProxySafetyViewModel(this._repository);

  final ProxySafetyRepository _repository;
  ProxySafetyStatus? status;
  bool isWorking = false;
  String? error;
  NetworkMatch networkMatch = NetworkMatch.unknown;
  Timer? _refreshTimer;
  Timer? _companionTimer;

  Future<void> load() async {
    await _run(_repository.status);
    _syncRefreshTimer();
  }

  Future<void> startMonitoring(String host, String portText) async {
    final port = int.tryParse(portText);
    if (host.trim().isEmpty || port == null || port < 1 || port > 65535) {
      error = 'Enter a reachable desktop host and a port from 1 to 65535.';
      notifyListeners();
      return;
    }
    await _run(
        () => _repository.startMonitoring(host: host.trim(), port: port));
    _syncRefreshTimer();
  }

  Future<void> stopMonitoring() async {
    await _run(_repository.stopMonitoring);
    _syncRefreshTimer();
  }

  Future<void> startVpn(String host, String portText, String targetPackage) async {
    final port = int.tryParse(portText);
    if (host.trim().isEmpty || port == null || port < 1 || port > 65535 || targetPackage.trim().isEmpty) {
      error = 'Enter a desktop host, a port from 1 to 65535, and the selected package.';
      notifyListeners();
      return;
    }
    await _run(() => _repository.startVpn(host: host.trim(), port: port, targetPackage: targetPackage.trim()));
    _syncRefreshTimer();
  }

  Future<void> connectFromQr(String rawPayload) async {
    if (isWorking) return;
    try {
      final payload = jsonDecode(rawPayload);
      if (payload is! Map<String, dynamic> ||
          payload['protocol'] != 'app-tester-companion') {
        throw const FormatException('This is not an App Tester connection code.');
      }
      if (payload['version'] != 2) {
        throw const FormatException(
            'Connection code requires another companion version. Update App Tester Companion, then scan again.');
      }
      final host = payload['host'];
      final port = payload['port'];
      final token = payload['token'];
      if (host is! String || port is! int || token is! String) {
        throw const FormatException(
            'Connection code is missing pairing data. Update both App Tester apps, then scan a newly generated code.');
      }
      networkMatch = await _networkMatch(host, port);
      if (networkMatch == NetworkMatch.unreachable) {
        error = 'Desktop is not reachable. Connect phone and computer to the same Wi-Fi, then scan again.';
        notifyListeners();
        return;
      }
      final apps = await _repository.launchableApps();
      final client = HttpClient();
      final registration = await client.post(host, port, '/__app_tester/companion/register');
      registration.headers.contentType = ContentType.json;
      registration.write(jsonEncode({'token': token, 'apps': apps}));
      final response = await registration.close();
      await response.drain<void>();
      if (response.statusCode != HttpStatus.ok) {
        throw const HttpException('Desktop rejected companion registration.');
      }
      client.close();
      await _run(() => _repository.startMonitoring(host: host, port: port));
      _companionTimer?.cancel();
      _companionTimer = Timer.periodic(const Duration(seconds: 1), (_) async {
        if (status?.isVpnActive == true || isWorking) return;
        final pollClient = HttpClient();
        try {
          final request = await pollClient.get(host, port, '/__app_tester/companion/config?token=$token');
          final result = await request.close();
          final config = jsonDecode(await utf8.decoder.bind(result).join());
          final package = config['package_name'];
          if (package is String && package.isNotEmpty) {
            _companionTimer?.cancel();
            await startVpn(host, port.toString(), package);
          }
        } finally {
          pollClient.close();
        }
      });
    } on FormatException catch (exception) {
      error = exception.message;
      notifyListeners();
    } catch (_) {
      error = 'Could not read this connection code. Scan the code shown by App Tester.';
      notifyListeners();
    }
  }

  Future<NetworkMatch> _networkMatch(String host, int port) async {
    try {
      final socket = await Socket.connect(host, port,
          timeout: const Duration(seconds: 2));
      await socket.close();
    } catch (_) {
      return NetworkMatch.unreachable;
    }
    final hostParts = host.split('.');
    if (hostParts.length != 4) {
      return NetworkMatch.reachable;
    }
    final interfaces = await NetworkInterface.list(type: InternetAddressType.IPv4);
    final sameSubnet = interfaces.expand((item) => item.addresses).any((address) {
      final parts = address.address.split('.');
      return parts.length == 4 &&
          parts.take(3).join('.') == hostParts.take(3).join('.');
    });
    return sameSubnet ? NetworkMatch.sameWifi : NetworkMatch.reachable;
  }

  Future<void> stopVpn() async {
    await _run(_repository.stopVpn);
    _syncRefreshTimer();
  }

  void _syncRefreshTimer() {
    _refreshTimer?.cancel();
    _companionTimer?.cancel();
    if (status?.isMonitoring != true &&
        status?.isVpnActive != true &&
        status?.targetPackage == null) {
      return;
    }
    _refreshTimer = Timer.periodic(
      const Duration(seconds: 5),
      (_) => _run(_repository.status),
    );
  }

  @override
  void dispose() {
    _refreshTimer?.cancel();
    super.dispose();
  }

  Future<void> _run(Future<ProxySafetyStatus> Function() action) async {
    isWorking = true;
    error = null;
    notifyListeners();
    try {
      status = await action();
    } catch (exception) {
      error = exception.toString();
    } finally {
      isWorking = false;
      notifyListeners();
    }
  }
}

enum NetworkMatch { unknown, sameWifi, reachable, unreachable }
