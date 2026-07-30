import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';

import '../data/proxy_safety_repository.dart';

typedef CompanionRegistration = Future<void> Function(
    String host, int port, String token, List<Map<String, String>> apps);

class ProxySafetyViewModel extends ChangeNotifier {
  ProxySafetyViewModel(this._repository, {CompanionRegistration? registerCompanion})
      : _registerCompanion = registerCompanion ?? _registerWithRetry;

  final ProxySafetyRepository _repository;
  final CompanionRegistration _registerCompanion;
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
    isWorking = true;
    error = null;
    networkMatch = NetworkMatch.unknown;
    notifyListeners();
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
      notifyListeners();
      final apps = await _repository.installedDebugApps();
      await _registerCompanion(host, port, token, apps);
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
    } on StateError catch (exception) {
      error = exception.message;
    } catch (_) {
      error = 'Could not connect to App Tester. Keep both apps open and scan the code again.';
    } finally {
      isWorking = false;
      notifyListeners();
    }
  }

  static Future<void> _registerWithRetry(
      String host, int port, String token, List<Map<String, String>> apps) async {
    Object? lastError;
    for (var attempt = 0; attempt < 3; attempt++) {
      final client = HttpClient()..connectionTimeout = const Duration(seconds: 2);
      try {
        final registration = await client.post(host, port, '/__app_tester/companion/register');
        registration.headers.contentType = ContentType.json;
        registration.write(jsonEncode({'token': token, 'apps': apps}));
        final response = await registration.close();
        await response.drain<void>();
        if (response.statusCode == HttpStatus.ok) return;
        lastError = HttpException('Desktop rejected companion registration.');
      } catch (error) {
        lastError = error;
      } finally {
        client.close(force: true);
      }
      if (attempt < 2) await Future<void>.delayed(const Duration(milliseconds: 600));
    }
    throw StateError('Desktop did not respond yet. Keep both apps open; the companion will retry when you scan again. ${lastError ?? ""}'.trim());
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

  Future<void> disconnect() async {
    _companionTimer?.cancel();
    if (status?.isVpnActive == true) {
      await _run(_repository.stopVpn);
    }
    await _run(_repository.stopMonitoring);
    networkMatch = NetworkMatch.unknown;
    _syncRefreshTimer();
    notifyListeners();
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
